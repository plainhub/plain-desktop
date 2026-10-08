use crate::{
    db::Db,
    library::{LibraryError, LibraryResult},
    utils::html_to_markdown::dom::Dom,
    utils::http_url::{join, parse_http_url},
};
use futures_util::StreamExt;
use std::{path::PathBuf, sync::Arc};
pub struct FeedAssets {
    pub db: Arc<Db>,
    pub directory: PathBuf,
}
impl FeedAssets {
    pub async fn cache(&self, url: &str) -> LibraryResult<String> {
        self.cache_formats(url, false).await
    }
    pub async fn cache_bookmark_icon(&self, url: &str) -> LibraryResult<String> {
        self.cache_formats(url, true).await
    }
    async fn cache_formats(&self, url: &str, bookmark: bool) -> LibraryResult<String> {
        let response = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| LibraryError::Other(e.to_string()))?
            .get(url)
            .send()
            .await
            .map_err(|e| LibraryError::Other(e.to_string()))?
            .error_for_status()
            .map_err(|e| LibraryError::Other(e.to_string()))?;
        let mime = response
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .unwrap_or_default()
            .split(';')
            .next()
            .unwrap_or_default()
            .to_lowercase();
        if !["image/png", "image/jpeg", "image/webp"].contains(&mime.as_str())
            && !(bookmark
                && ["image/x-icon", "image/vnd.microsoft.icon", "image/svg+xml"]
                    .contains(&mime.as_str()))
        {
            return Err(LibraryError::Other("unsupported feed image format".into()));
        }
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| LibraryError::Other(e.to_string()))?;
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                return Err(LibraryError::Other("feed image too large".into()));
            }
            bytes.extend_from_slice(&chunk);
        }
        let db = self.db.clone();
        let directory = self.directory.clone();
        let file = tokio::task::spawn_blocking(move || {
            crate::chat::app_file_store::import_bytes(&db, &directory, &bytes, &mime)
        })
        .await
        .map_err(|e| LibraryError::Other(e.to_string()))?
        .map_err(|e| LibraryError::Other(e.to_string()))?;
        Ok(file.real_path.to_string_lossy().into_owned())
    }
    pub fn release(&self, uri: &str) {
        let path = std::path::Path::new(uri);
        let Some(id) = path.file_stem().and_then(|v| v.to_str()) else {
            return;
        };
        let Some(file) = self.db.get_app_file(id) else {
            return;
        };
        let owned = self.directory.join(&file.real_path);
        if path != owned || !owned.starts_with(self.directory.join("files")) {
            return;
        }
        if let Err(error) = crate::chat::app_file_store::release(&self.db, &self.directory, id) {
            log::warn!("feed asset release failed: {error}");
        }
    }
}
/// Resolves `reference` against `base`, keeping only http(s). A `javascript:`
/// or `data:` reference must stay as it is instead of turning into a
/// site-relative path.
pub fn absolute_url(base: &str, reference: &str) -> Option<String> {
    let reference = reference.trim();
    if reference.is_empty() {
        return None;
    }
    if let Some(scheme) = scheme_of(reference) {
        return ["http", "https"]
            .contains(&scheme.as_str())
            .then(|| reference.to_string());
    }
    if let Some(rest) = reference.strip_prefix("//") {
        return Some(format!("{}://{rest}", parse_http_url(base)?.scheme));
    }
    join(base, reference)
}

fn scheme_of(reference: &str) -> Option<String> {
    let colon = reference.find(':')?;
    let scheme = &reference[..colon];
    let valid = !scheme.is_empty()
        && !scheme.contains('/')
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.');
    valid.then(|| scheme.to_ascii_lowercase())
}

/// Rewrites relative `src`/`href` references to absolute ones, so article
/// HTML keeps working after it is read outside the page it came from.
pub fn normalize_html(html: &str, base: &str) -> String {
    let mut dom = Dom::parse(html);
    for node in &mut dom.0 {
        for (name, value) in node.attrs.iter_mut() {
            if matches!(name.as_str(), "src" | "href")
                && let Some(url) = absolute_url(base, value)
            {
                *value = url;
            }
        }
    }
    super::inner_html(&dom, 0)
}

pub fn main_image(html: &str, base: &str) -> Option<String> {
    Dom::parse(html)
        .0
        .iter()
        .filter(|node| node.tag == "img")
        .find_map(|node| absolute_url(base, node.attr("src")))
}
