use super::server::ServerState;
use crate::db::DPeer;
use anyhow::{Result, ensure};
use axum::{
    Json,
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
pub(super) struct Lan {
    pub(super) client: reqwest::Client,
    pub(super) capacity: Arc<tokio::sync::Semaphore>,
}
impl Lan {
    pub(super) fn new() -> Result<Self> {
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
pub(super) enum Failure {
    Unavailable(String),
    Fatal(String),
}
pub(super) async fn send(
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
    let peer = super::peer_address::current(&state.db, &peer.id, peer)
        .map_err(|e| Failure::Fatal(e.to_string()))?;
    let actor = state
        .prefs
        .get::<String>("client_id")
        .map_err(|e| Failure::Fatal(e.to_string()))?
        .filter(|id| !id.is_empty())
        .ok_or_else(|| Failure::Fatal("Missing peer actor".into()))?;
    let cipher = crate::xchacha_encrypt_raw(key, body.as_bytes())
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
    let plain = crate::xchacha_decrypt_raw(key, &bytes)
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
pub(super) async fn limited(response: reqwest::Response, limit: usize) -> Result<Vec<u8>> {
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
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FileRequest {
    id: String,
    expected: DPeer,
    file_id: String,
}
pub(super) async fn file(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<FileRequest>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if *state.stop.borrow() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let mut stop = state.stop.clone();
    let body_stop = stop.clone();
    let work = async {
        let permit = state.lan.capacity.clone().acquire_owned().await?;
        let peer = super::peer_address::current(&state.db, &request.id, &request.expected)?;
        let url = super::peer_address::file_url(&peer, &request.file_id)?;
        let actor = state
            .prefs
            .get::<String>("client_id")?
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Missing peer actor"))?;
        let response = tokio::time::timeout(
            Duration::from_secs(10),
            state.lan.client.get(url).header("c-id", actor).send(),
        )
        .await??;
        ensure!(
            response.status().is_success(),
            "Peer file HTTP {}",
            response.status()
        );
        super::peer_address::current(&state.db, &request.id, &request.expected)?;
        streaming(response, permit, body_stop)
    };
    match tokio::select! {_=stop.changed()=>Err(anyhow::anyhow!("Core stopped")),result=work=>result}
    {
        Ok(response) => response,
        Err(error) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error":error.to_string()})),
        )
            .into_response(),
    }
}
pub(super) fn streaming(
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
#[path = "../../tests/unit/content_api/peer_lan.rs"]
mod tests;
