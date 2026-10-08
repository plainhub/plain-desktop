use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Link {
    pub host: String,
    pub port: u16,
    pub shared_id: String,
    pub token: String,
    #[serde(default)]
    pub page_url: String,
}
impl Link {
    pub fn new(host: &str, port: u16, id: &str, token: &str) -> Result<Self> {
        let mut link = Self {
            host: host.trim().into(),
            port,
            shared_id: id.into(),
            token: token.into(),
            page_url: String::new(),
        };
        let mut url = link.url("/")?;
        url.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("Invalid share URL"))?
            .clear()
            .push("s")
            .push(id);
        url.set_fragment(Some(token));
        link.page_url = url.to_string();
        Ok(link)
    }
    pub fn url(&self, path: &str) -> Result<reqwest::Url> {
        ensure!(
            self.port > 0 && !self.host.is_empty() && !self.shared_id.is_empty(),
            "Invalid share endpoint"
        );
        ensure!(
            !self
                .host
                .chars()
                .any(|c| c.is_whitespace() || "/@?#,\\".contains(c)),
            "Invalid share host"
        );
        let url = reqwest::Url::parse(&crate::utils::build_url::build_url(
            "https", &self.host, self.port, path,
        ))?;
        ensure!(
            url.username().is_empty()
                && url.password().is_none()
                && url.scheme() == "https"
                && url.host_str().is_some(),
            "Invalid share URL"
        );
        self.key()?;
        Ok(url)
    }
    pub fn key(&self) -> Result<Vec<u8>> {
        let key = crate::utils::base64::base64_decode_checked(&self.token)?;
        ensure!(key.len() == 32, "Invalid share token");
        Ok(key)
    }
    pub fn request(&self, virtual_path: Option<&str>) -> Result<Vec<u8>> {
        let query = "query($virtualPath: String) { sharedInfo(virtualPath: $virtualPath) { name readOnly requiresPassword expiresAt urlToken entries { name virtualPath isDir size mimeType hasThumb } } }";
        let body = json!({"query":query,"variables":{"virtualPath":virtual_path}}).to_string();
        let envelope = format!(
            "{}|{}|{body}",
            crate::chat::pairing::now_ms(),
            uuid::Uuid::new_v4()
        );
        crate::xchacha_encrypt_raw(&self.key()?, envelope.as_bytes())
            .ok_or_else(|| anyhow::anyhow!("Cannot encrypt shared request"))
    }
    pub fn response(&self, bytes: &[u8]) -> Result<Info> {
        let plain = crate::xchacha_decrypt_raw(&self.key()?, bytes)
            .ok_or_else(|| anyhow::anyhow!("Invalid shared response authentication"))?;
        let response: Value = serde_json::from_slice(&plain)?;
        if let Some(error) = response
            .get("errors")
            .and_then(Value::as_array)
            .and_then(|errors| errors.first())
        {
            anyhow::bail!(
                "{}",
                error["message"].as_str().unwrap_or("Share request failed")
            );
        }
        let info: Info = serde_json::from_value(response["data"]["sharedInfo"].clone())?;
        ensure!(
            crate::utils::base64::base64_decode_checked(&info.url_token)?.len() == 32,
            "Invalid shared file token"
        );
        Ok(info)
    }
    pub fn file_url(&self, url_token: &str, virtual_path: &str, zip: bool) -> Result<String> {
        let key = crate::utils::base64::base64_decode_checked(url_token)?;
        ensure!(key.len() == 32, "Invalid shared file token");
        let bytes = crate::xchacha_encrypt_raw(
            &key,
            &serde_json::to_vec(&json!({"sharedId":self.shared_id,"virtualPath":virtual_path}))?,
        )
        .ok_or_else(|| anyhow::anyhow!("Cannot encrypt shared file ID"))?;
        let mut url = self.url(if zip { "/zip/dir" } else { "/fs" })?;
        url.query_pairs_mut()
            .append_pair("sid", &self.shared_id)
            .append_pair("id", &crate::base64_encode(&bytes));
        Ok(url.to_string())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct File {
    pub name: String,
    pub virtual_path: String,
    pub is_dir: bool,
    pub size: i64,
    pub mime_type: String,
    pub has_thumb: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub name: String,
    pub read_only: bool,
    pub requires_password: bool,
    pub expires_at: Option<i64>,
    pub url_token: String,
    pub entries: Vec<File>,
}
#[cfg(test)]
#[path = "../../tests/unit/shares/client.rs"]
mod tests;
