use crate::assembly::server::ServerState;
use anyhow::{Result, ensure};
use axum::{
    body::Body,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use plain_rs::db::DPeer;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
pub(crate) struct Lan {
    pub(crate) client: reqwest::Client,
    pub(crate) capacity: Arc<tokio::sync::Semaphore>,
}
impl Lan {
    pub(crate) fn new() -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .use_rustls_tls()
                .danger_accept_invalid_certs(true)
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(3))
                .build()?,
            capacity: Arc::new(tokio::sync::Semaphore::new(4)),
        })
    }
}
#[derive(Debug)]
pub(crate) enum Failure {
    Unavailable(String),
    Fatal(String),
}
pub(crate) async fn send(
    state: &ServerState,
    peer: &DPeer,
    channel: &str,
    key: &[u8],
    body: &str,
    validate: impl Fn() -> Result<(), String>,
) -> Result<Value, Failure> {
    let _permit = state
        .lan
        .capacity
        .clone()
        .acquire_owned()
        .await
        .map_err(|e| Failure::Fatal(e.to_string()))?;
    validate().map_err(Failure::Fatal)?;
    let peer = crate::services::peers::peer_address::current(&state.db, &peer.id, peer)
        .map_err(|e| Failure::Fatal(e.to_string()))?;
    let actor = state
        .prefs
        .get::<String>(plain_rs::prefs::system::keys::CLIENT_ID)
        .map_err(|e| Failure::Fatal(e.to_string()))?
        .filter(|id| !id.is_empty())
        .ok_or_else(|| Failure::Fatal("Missing peer actor".into()))?;
    let cipher = plain_rs::crypto::xchacha_encrypt_raw(key, body.as_bytes())
        .ok_or_else(|| Failure::Fatal("Invalid peer encryption key".into()))?;
    let response = state
        .lan
        .client
        .post(peer.peer_graphql_url())
        .timeout(Duration::from_secs(10))
        .header("content-type", "application/octet-stream")
        .header("c-id", actor)
        .header("c-cid", channel)
        .body(cipher)
        .send()
        .await
        .map_err(|e| Failure::Unavailable(e.to_string()))?;
    let status = response.status();
    let bytes = limited(response, 4 * 1024 * 1024)
        .await
        .map_err(|e| Failure::Fatal(e.to_string()))?;
    let plain = plain_rs::crypto::xchacha_decrypt_raw(key, &bytes)
        .ok_or_else(|| Failure::Fatal("Failed to authenticate peer response".into()))?;
    if !status.is_success() {
        return Ok(json!({"data":null,"errors":[{"message":format!("Peer HTTP {status}")}]}));
    }
    let response: Value =
        serde_json::from_slice(&plain).map_err(|e| Failure::Fatal(e.to_string()))?;
    if !response.is_object() {
        return Err(Failure::Fatal("Peer response is not a JSON object".into()));
    }
    Ok(response)
}
pub(crate) async fn limited(response: reqwest::Response, limit: usize) -> Result<Vec<u8>> {
    ensure!(
        response.content_length().is_none_or(|n| n <= limit as u64),
        "Peer response exceeds limit"
    );
    let mut chunks = response.bytes_stream();
    let mut body = vec![];
    while let Some(bytes) = chunks.next().await {
        let bytes = bytes?;
        ensure!(
            body.len() + bytes.len() <= limit,
            "Peer response exceeds limit"
        );
        body.extend_from_slice(&bytes);
    }
    Ok(body)
}
pub(crate) fn streaming(
    response: reqwest::Response,
    permit: tokio::sync::OwnedSemaphorePermit,
    stop: tokio::sync::watch::Receiver<bool>,
) -> Result<Response> {
    let status = StatusCode::from_u16(response.status().as_u16())?;
    let length = response.content_length();
    let stream = futures_util::stream::unfold(
        (response.bytes_stream(), permit, stop),
        |(mut stream, permit, mut stop)| async move {
            let chunk = tokio::select! { _=stop.changed()=>None,chunk=tokio::time::timeout(Duration::from_secs(30),stream.next())=>match chunk { Ok(chunk)=>chunk.map(|chunk|chunk.map_err(std::io::Error::other)), Err(_)=>Some(Err(std::io::Error::new(std::io::ErrorKind::TimedOut,"Peer file stalled"))) } };
            chunk.map(|chunk| (chunk, (stream, permit, stop)))
        },
    );
    let mut response = (status, Body::from_stream(stream)).into_response();
    if let Some(length) = length {
        response
            .headers_mut()
            .insert("content-length", length.to_string().parse()?);
    }
    Ok(response)
}
#[cfg(all(test, feature = "http_transport"))]
#[path = "../../../tests/unit/services/peers/peer_lan.rs"]
mod tests;
