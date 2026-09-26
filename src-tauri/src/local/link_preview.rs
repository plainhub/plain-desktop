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

use regex::Regex;
use serde_json::{Value, json};

use plain_rs::utils::image_dimensions;

use crate::local::db::ChatDb;
use plain_rs::chat::app_file_store::import_bytes;

/// Maximum HTML response body we will parse.
const MAX_RESPONSE_SIZE: usize = 10 * 1024 * 1024; // 10MB
/// Maximum preview-image payload we will import.
const MAX_IMAGE_SIZE: usize = 5 * 1024 * 1024; // 5MB
/// Fetch timeout for both the page and its preview image.
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);

fn url_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)https?://(?:[-\w.])+(?:\:[0-9]+)?(?:/(?:[\w/_.-]*(?:\?[\w&=%.+-]*)?(?:#[\w.-]*)?)?)?")
            .expect("url regex")
    })
}

/// Extract up to 5 distinct valid URLs from `text`. Mirrors
/// `LinkPreviewHelper.extractUrls`.
pub fn extract_urls(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut urls = Vec::new();
    for m in url_regex().find_iter(text) {
        let url = m.as_str();
        if is_valid_url(url) && seen.insert(url.to_string()) {
            urls.push(url.to_string());
            if urls.len() == 5 {
                break;
            }
        }
    }
    urls
}

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
fn is_valid_url(url: &str) -> bool {
    let without_protocol = url.split_once("://").map(|(_, r)| r).unwrap_or("");
    if without_protocol.is_empty() {
        return false;
    }
    let host = without_protocol
        .split(['/', ':'])
        .next()
        .unwrap_or("")
        .to_lowercase();
    if host.is_empty() {
        return false;
    }
    if host == "localhost"
        || host.starts_with("127.")
        || host.starts_with("192.168.")
        || host.starts_with("10.")
    {
        return false;
    }
    // 172.16.0.0/12 .. 172.31.255.255
    if host.starts_with("172.") {
        let octet: u16 = host
            .split('.')
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        if (16..=31).contains(&octet) {
            return false;
        }
    }
    true
}

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
    reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .build()
        .ok()
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
async fn fetch_link_preview(db: &ChatDb, data_dir: &Path, url: &str) -> Value {
    let Some(client) = http_client() else {
        return error_preview(url);
    };
    let response = match client.get(url).send().await {
        Ok(r) => r,
        Err(_) => return error_preview(url),
    };
    if !response.status().is_success() {
        return error_preview(url);
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();
    if !content_type.contains("text/html") {
        return error_preview(url);
    }
    if response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok())
        .is_some_and(|len| len > MAX_RESPONSE_SIZE)
    {
        return error_preview(url);
    }
    let html = match response.text().await {
        Ok(t) => t,
        Err(_) => return error_preview(url),
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

    let mut image_local_path: Option<String> = None;
    let mut image_width = 0;
    let mut image_height = 0;
    if let Some(active_url) = image_url.as_deref()
        && is_valid_url(active_url)
    {
        let (path, w, h) = download_image_with_size(db, data_dir, active_url).await;
        image_local_path = path;
        image_width = w;
        image_height = h;
        if image_local_path.is_none() && active_url.ends_with("/favicon.ico") {
            image_url = None;
        }
    }

    json!({
        "url": url,
        "title": title.and_then(|t| optional(&t).map(|s| s.chars().take(200).collect::<String>())),
        "description": description.and_then(|d| optional(&d).map(|s| s.chars().take(300).collect::<String>())),
        "imageUrl": image_url.and_then(|u| optional(&u)),
        "imageLocalPath": image_local_path,
        "imageWidth": image_width,
        "imageHeight": image_height,
        "siteName": site_name.and_then(|s| optional(&s).map(|v| v.chars().take(100).collect::<String>())),
        "domain": optional(&domain),
        "hasError": false,
    })
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
    db: &ChatDb,
    data_dir: &Path,
    image_url: &str,
) -> (Option<String>, i32, i32) {
    let Some(client) = http_client() else {
        return (None, 0, 0);
    };
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
    let bytes = match response.bytes().await {
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
    let path = import_bytes(db, data_dir, &bytes, &content_type)
        .ok()
        .map(|r| format!("fid:{}", r.fid_suffix));
    (path, w, h)
}

/// Rewrite a text chat item's `content` JSON to add `linkPreviews` for any
/// URLs found in its text. Returns the updated content string, or `None`
/// when there is nothing to change (non-text, no URLs, or no previews
/// resolved). Existing previews are preserved; only the URLs that are both
/// new and resolvable are appended.
pub async fn ensure_link_previews(db: &ChatDb, data_dir: &Path, content: &str) -> Option<String> {
    let mut v: Value = serde_json::from_str(content).ok()?;
    let type_uppercase = v.get("type").and_then(|t| t.as_str())?.to_uppercase();
    if type_uppercase != "TEXT" {
        return None;
    }
    let text = v.pointer("/value/text").and_then(|t| t.as_str())?;
    let urls = extract_urls(text);
    if urls.is_empty() {
        return None;
    }

    let mut previews: Vec<Value> = v
        .pointer("/value/linkPreviews")
        .and_then(|p| p.as_array())
        .cloned()
        .unwrap_or_default();
    let existing: HashSet<String> = previews
        .iter()
        .filter_map(|p| p.get("url").and_then(|u| u.as_str()))
        .map(String::from)
        .collect();

    for url in &urls {
        if existing.contains(url) {
            continue;
        }
        let preview = fetch_link_preview(db, data_dir, url).await;
        if preview.get("hasError").and_then(|e| e.as_bool()) != Some(true) {
            previews.push(preview);
        }
    }
    if previews.is_empty() {
        return None;
    }

    v["value"]["linkPreviews"] = Value::Array(previews);
    serde_json::to_string(&v).ok()
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
