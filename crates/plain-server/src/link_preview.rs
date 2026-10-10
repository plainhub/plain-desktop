//! Link preview generation (port of `plain-app` `LinkPreview.kt` +
//! `LinkPreviewHelper.kt`).
//!
//! For a text chat item, detects URLs in the message, fetches each one,
//! extracts title / description / site name, downloads a preview image into
//! the content-addressable `app_files` store (`fid:` URI), and rewrites the
//! stored `content` JSON with a `linkPreviews` array. The refreshed content
//! is then broadcast to the web client via `WS_MESSAGE_UPDATED` by the
//! caller.
//!
//! Everything is best-effort: a URL that fails to fetch or lacks usable
//! metadata simply produces no preview entry.

use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use futures_util::StreamExt;
use regex::Regex;
use serde_json::{Value, json};

use crate::utils::image_dimensions;

use crate::chat::link_preview as commit;
pub use crate::chat::link_preview::extract_urls;
use crate::chat::link_preview::is_valid_url;
mod schedule;
use crate::chat::app_file_store::import_preview_image;
use crate::db::Db;
pub use commit::{Edit, edit};
pub use schedule::Schedule;

/// Maximum HTML response body we will parse.
const MAX_RESPONSE_SIZE: usize = 10 * 1024 * 1024; // 10MB
/// Maximum preview-image payload we will import.
const MAX_IMAGE_SIZE: usize = 5 * 1024 * 1024; // 5MB
/// Fetch timeout for both the page and its preview image.
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);

/// Resolve a possibly-relative image URL against `base`. Mirrors
/// `LinkPreviewHelper.resolveUrl`.
fn resolve_url(base: &str, url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        return url.to_string();
    }
    let protocol = base.split_once("://").map(|(p, _)| p).unwrap_or("https");
    let rest = base.split_once("://").map(|(_, r)| r).unwrap_or("");
    if url.starts_with("//") {
        return format!("{protocol}:{url}");
    }
    let host = rest.split('/').next().unwrap_or("");
    let (host_only, port_part) = match host.split_once(':') {
        Some((h, port)) => (h, format!(":{port}")),
        None => (host, String::new()),
    };
    if url.starts_with('/') {
        return format!("{protocol}://{host_only}{port_part}{url}");
    }
    let dir = rest.rsplit_once('/').map(|(d, _)| d).unwrap_or(rest);
    let base_path = match dir.split_once(host) {
        Some((_, after)) => after,
        None => dir,
    };
    format!("{protocol}://{host_only}{port_part}{base_path}/{url}")
}

/// Reject URLs whose host is a loopback / private-LAN address. Mirrors
/// `LinkPreviewHelper.isValidUrl`.
/// Host part of `url`, or empty when unparsable. Mirrors
/// `LinkPreview.extractHost` (ktor `Url(url).host`).
fn extract_host(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(String::from))
        .unwrap_or_default()
}

/// Attempt a shared fetch client (per call this is cheap; reqwest caches
/// connections internally).
fn http_client() -> Option<reqwest::Client> {
    static CLIENT: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .timeout(FETCH_TIMEOUT)
                .build()
                .ok()
        })
        .clone()
}

/// Trim a string, return `None` when it becomes empty. Mirrors the
/// `.ifEmpty { null }` used by `LinkPreview.kt`.
fn optional(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

/// Escape-rules light: find the value captured inside `content` of an HTML tag.
fn og_value(html: &str, pattern: &str) -> Option<String> {
    let re = Regex::new(&format!("(?i){pattern}")).ok()?;
    re.captures(html).map(|c| c[1].to_string())
}

/// `og:title` wins over the plain `<title>`; fall back to `<title>` when no
/// OG tag is present. Mirrors `LinkPreview.kt`.
fn og_title_or_default(html: &str) -> Option<String> {
    let og = og_value(
        html,
        "<meta[^>]+property=[\"']og:title[\"'][^>]+content=[\"']([^\"']+)[\"']",
    );
    og.or_else(|| og_value(html, "<title[^>]*>([^<]+)</title>"))
}

/// Fetch one URL and build a link-preview JSON object (same shape as
/// `plain-app` `DLinkPreview`). Any failure returns `{url, hasError: true}`.
async fn fetch_link_preview(client: &reqwest::Client, url: &str) -> (Value, Option<Image>) {
    let response = match client.get(url).send().await {
        Ok(r) => r,
        Err(_) => return (error_preview(url), None),
    };
    if !response.status().is_success() {
        return (error_preview(url), None);
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();
    if !content_type.contains("text/html") {
        return (error_preview(url), None);
    }
    if response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok())
        .is_some_and(|len| len > MAX_RESPONSE_SIZE)
    {
        return (error_preview(url), None);
    }
    let html = match read_bounded(response, MAX_RESPONSE_SIZE).await {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(_) => return (error_preview(url), None),
    };
    let domain = extract_host(url);

    let title = og_title_or_default(&html);
    let mut description = og_value(
        &html,
        "<meta[^>]+property=[\"']og:description[\"'][^>]+content=[\"']([^\"']+)[\"']",
    );
    if description.is_none() {
        description = og_value(
            &html,
            "<meta[^>]+name=[\"']description[\"'][^>]+content=[\"']([^\"']+)[\"']",
        );
    }
    let site_name = og_value(
        &html,
        "<meta[^>]+property=[\"']og:site_name[\"'][^>]+content=[\"']([^\"']+)[\"']",
    );

    let og_image = og_value(
        &html,
        "<meta[^>]+property=[\"']og:image[\"'][^>]+content=[\"']([^\"']+)[\"']",
    );
    let mut image_url = og_image.as_deref().map(|u| resolve_url(url, u.trim()));
    if image_url.is_none() {
        image_url = extract_favicon(&html, url);
    }
    if image_url.is_none() {
        image_url = reqwest::Url::parse(url)
            .ok()
            .filter(|u| !u.host_str().unwrap_or("").is_empty())
            .map(|u| {
                format!(
                    "{}://{}/favicon.ico",
                    u.scheme(),
                    u.host_str().unwrap_or("")
                )
            });
    }

    let mut image: Option<Image> = None;
    let mut image_width = 0;
    let mut image_height = 0;
    if let Some(active_url) = image_url.as_deref()
        && is_valid_url(active_url)
    {
        let (downloaded, w, h) = download_image_with_size(client, active_url).await;
        image = downloaded;
        image_width = w;
        image_height = h;
        if image.is_none() && active_url.ends_with("/favicon.ico") {
            image_url = None;
        }
    }

    (
        json!({
            "url": url,
            "title": title.and_then(|t| optional(&t).map(|s| s.chars().take(200).collect::<String>())),
            "description": description.and_then(|d| optional(&d).map(|s| s.chars().take(300).collect::<String>())),
            "imageUrl": image_url.and_then(|u| optional(&u)),
            "imageLocalPath": Value::Null,
            "imageWidth": image_width,
            "imageHeight": image_height,
            "siteName": site_name.and_then(|s| optional(&s).map(|v| v.chars().take(100).collect::<String>())),
            "domain": optional(&domain),
            "hasError": false,
        }),
        image,
    )
}

fn error_preview(url: &str) -> Value {
    json!({ "url": url, "hasError": true })
}

/// Pull the first link/icon href, falling back to a `<favicon.ico>` guess.
/// Mirrors the favicon-pattern loop in `LinkPreview.kt`.
fn extract_favicon(html: &str, url: &str) -> Option<String> {
    const PATTERNS: [&str; 4] = [
        "<link[^>]+rel=[\"'][^\"']*icon[^\"']*[\"'][^>]+href=[\"']([^\"']+)[\"']",
        "<link[^>]+href=[\"']([^\"']+)[\"'][^>]+rel=[\"'][^\"']*icon[^\"']*[\"']",
        "<link[^>]+rel=[\"']shortcut icon[\"'][^>]+href=[\"']([^\"']+)[\"']",
        "<link[^>]+rel=[\"']apple-touch-icon[^\"']*[\"'][^>]+href=[\"']([^\"']+)[\"']",
    ];
    for pattern in PATTERNS {
        if let Some(href) = og_value(html, pattern) {
            return Some(resolve_url(url, href.trim()));
        }
    }
    None
}

/// Download a preview image, import it into the `app_files` store, and
/// return `(fid:..., width, height)` (empty path on failure). Mirrors
/// `downloadImageWithSize` + `importImageBytesToFid`.
async fn download_image_with_size(
    client: &reqwest::Client,
    image_url: &str,
) -> (Option<Image>, i32, i32) {
    let response = match client.get(image_url).send().await {
        Ok(r) => r,
        Err(_) => return (None, 0, 0),
    };
    if !response.status().is_success() {
        return (None, 0, 0);
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();
    let is_favicon_file = image_url.contains("favicon") || image_url.ends_with(".ico");
    let is_image_ctype = content_type.starts_with("image/")
        || (is_favicon_file
            && (content_type.contains("icon") || content_type.contains("octet-stream")));
    if !is_image_ctype {
        return (None, 0, 0);
    }
    let bytes = match read_bounded(response, MAX_IMAGE_SIZE).await {
        Ok(b) => b,
        Err(_) => return (None, 0, 0),
    };
    if bytes.len() > MAX_IMAGE_SIZE {
        return (None, 0, 0);
    }
    let (w, h) = image_dimensions::dimensions(&bytes).unwrap_or((0, 0));
    let is_favicon = image_url.contains("favicon")
        || image_url.contains("icon")
        || (w < 200 && h < 200 && w > 16 && h > 16);
    if (w < 100 || h < 100) && !is_favicon {
        return (None, w, h);
    }
    (
        Some(Image {
            bytes,
            mime: content_type,
        }),
        w,
        h,
    )
}

struct Image {
    bytes: Vec<u8>,
    mime: String,
}
async fn read_bounded(mut response: reqwest::Response, limit: usize) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            anyhow::bail!("Preview response too large");
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
pub async fn refresh(
    db: &Db,
    directory: &Path,
    id: &str,
) -> anyhow::Result<Option<crate::db::DChat>> {
    let Some(client) = http_client() else {
        return Ok(None);
    };
    refresh_with(db, directory, id, &client).await
}
async fn refresh_with(
    db: &Db,
    directory: &Path,
    id: &str,
    client: &reqwest::Client,
) -> anyhow::Result<Option<crate::db::DChat>> {
    let Some(row) = crate::db::chat_store::messages::get(db, id)? else {
        return Ok(None);
    };
    let content: Value = serde_json::from_str(&row.content)?;
    if content["type"] != "TEXT" {
        return Ok(None);
    };
    let text = content["value"]["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid text message"))?;
    let existing = content["value"]["linkPreviews"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p["url"].as_str())
        .collect::<HashSet<_>>();
    let mut updated = None;
    let urls = extract_urls(text)
        .into_iter()
        .filter(|url| !existing.contains(url.as_str()))
        .collect::<Vec<_>>();
    let mut fetched = futures_util::stream::iter(
        urls.into_iter()
            .map(|url| async move { fetch_link_preview(client, &url).await }),
    )
    .buffered(5);
    while let Some((preview, image)) = fetched.next().await {
        if preview["hasError"] == true {
            continue;
        }
        let db = db.clone();
        let directory = directory.to_path_buf();
        let id = id.to_owned();
        let text = text.to_owned();
        let result = tokio::task::spawn_blocking(move || match image {
            Some(image) => Ok(import_preview_image(
                &db,
                &directory,
                &image.bytes,
                &image.mime,
                &id,
                &text,
                &preview,
            )?
            .chat),
            None => commit::append(&db, &id, &text, &preview),
        })
        .await??;
        if result.is_none() {
            break;
        }
        updated = result;
    }
    Ok(updated)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_urls_finds_and_dedups() {
        let urls = extract_urls("see https://a.com/x and https://a.com/x then https://b.org");
        assert_eq!(
            urls,
            vec!["https://a.com/x".to_string(), "https://b.org".to_string()]
        );
    }

    #[test]
    fn extract_urls_limits_to_five() {
        let text = (0..8)
            .map(|i| format!(" https://c{i}.com"))
            .collect::<String>();
        assert_eq!(extract_urls(&text).len(), 5);
    }

    #[test]
    fn extract_urls_rejects_private_hosts() {
        assert!(extract_urls("http://192.168.1.5/x").is_empty());
        assert!(extract_urls("http://127.0.0.1").is_empty());
        assert!(extract_urls("http://10.0.0.2").is_empty());
        assert!(extract_urls("http://172.20.3.4").is_empty());
        assert!(extract_urls("http://172.40.3.4/x").len() == 1);
    }

    #[test]
    fn resolve_url_handles_relative_forms() {
        assert_eq!(
            resolve_url("https://a.com/x/y", "https://b.com/z"),
            "https://b.com/z"
        );
        assert_eq!(
            resolve_url("https://a.com/x/y", "//cdn.com/img.png"),
            "https://cdn.com/img.png"
        );
        assert_eq!(
            resolve_url("https://a.com/x/y", "/img.png"),
            "https://a.com/img.png"
        );
        assert_eq!(
            resolve_url("https://a.com/x/y", "img.png"),
            "https://a.com/x/img.png"
        );
    }
}

#[cfg(test)]
#[path = "../tests/unit/link_preview/fetch.rs"]
mod fetch_tests;
