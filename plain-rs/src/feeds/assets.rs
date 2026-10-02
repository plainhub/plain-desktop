use crate::{
    db::Db,
    library::{LibraryError, LibraryResult},
};
use futures_util::StreamExt;
use std::{path::PathBuf, sync::Arc};
pub struct FeedAssets {
    pub db: Arc<Db>,
    pub directory: PathBuf,
}
impl FeedAssets {
    pub async fn cache(&self, url: &str) -> LibraryResult<String> {
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
        if !["image/png", "image/jpeg", "image/webp"].contains(&mime.as_str()) {
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
        if self.db.decrement_app_file_ref(id) <= 0 {
            let _ = std::fs::remove_file(owned);
            self.db.delete_app_file(id);
        }
    }
}
pub fn absolute_url(base: &str, reference: &str) -> Option<String> {
    reqwest::Url::parse(base)
        .ok()?
        .join(reference)
        .ok()
        .filter(|u| ["http", "https"].contains(&u.scheme()))
        .map(|u| u.to_string())
}
pub fn normalize_html(html: &str, base: &str) -> String {
    let regex = regex::Regex::new(r#"(?is)(src|href)\s*=\s*["']([^"']+)["']"#).unwrap();
    regex
        .replace_all(html, |captures: &regex::Captures| {
            absolute_url(base, &captures[2])
                .map(|url| format!("{}=\"{}\"", &captures[1], url))
                .unwrap_or_else(|| captures[0].to_string())
        })
        .into_owned()
}
pub fn main_image(html: &str, base: &str) -> Option<String> {
    let document = scraper::Html::parse_fragment(html);
    document
        .select(&scraper::Selector::parse("img[src]").unwrap())
        .filter_map(|e| absolute_url(base, e.value().attr("src")?))
        .next()
}
