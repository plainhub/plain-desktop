use super::pairing::protocol::{PairingCancel, PairingRequest, PairingResponse};
use anyhow::Result;
use serde::Deserialize;
use std::{
    net::{IpAddr, SocketAddr},
    sync::OnceLock,
    time::Duration,
};

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    content = "payload",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum Message {
    PairRequest(PairingRequest),
    PairResponse(PairingResponse),
    PairCancel(PairingCancel),
}
impl Message {
    pub fn wire(&self) -> Result<String> {
        Ok(match self {
            Self::PairRequest(value) => format!("PAIR_REQUEST:{}", serde_json::to_string(value)?),
            Self::PairResponse(value) => format!("PAIR_RESPONSE:{}", serde_json::to_string(value)?),
            Self::PairCancel(value) => format!("PAIR_CANCEL:{}", serde_json::to_string(value)?),
        })
    }
}
fn client() -> Result<&'static reqwest::Client> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .no_proxy()
                .danger_accept_invalid_certs(true)
                .danger_accept_invalid_hostnames(true)
                .connect_timeout(Duration::from_secs(5))
                .build()
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| anyhow::anyhow!(e.to_owned()))
}
fn url(ip: &str, port: u16) -> Result<String> {
    anyhow::ensure!(port != 0, "Missing nearby port");
    let ip: IpAddr = ip
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse()?;
    Ok(format!("https://{}/nearby", SocketAddr::new(ip, port)))
}
async fn post_url(url: &str, body: String, timeout: Duration) -> Result<bool> {
    let result = client()?
        .post(url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .timeout(timeout)
        .body(body)
        .send()
        .await;
    Ok(result.is_ok_and(|response| response.status().is_success()))
}
pub async fn send(ip: &str, port: u16, message: &Message) -> Result<bool> {
    post_url(&url(ip, port)?, message.wire()?, Duration::from_secs(5)).await
}
pub async fn probe(ip: &str, port: u16) -> Result<bool> {
    post_url(
        &url(ip, port)?,
        "DISCOVER:".into(),
        Duration::from_millis(2500),
    )
    .await
}
#[cfg(test)]
#[path = "../../tests/unit/chat/nearby_http.rs"]
mod tests;
