//! `GET /proxyfs` — proxies a peer HTTP URL for the main web UI.
//!
//! The `id` query parameter is a urlToken-encrypted absolute URL, matching
//! plain-app's `UrlHelper.decrypt` (base64 of the XChaCha20-Poly1305 blob).
//! Only the client that holds the urlToken can mint one, so the decrypted value
//! is a peer URL chosen by the UI, not attacker input.

use super::server::ServerState;
use axum::{
    body::Body,
    extract::{ConnectInfo, Query, State},
    http::{HeaderMap, HeaderName, StatusCode, header},
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use serde::Deserialize;
use std::net::SocketAddr;

#[derive(Deserialize)]
pub(super) struct Params {
    #[serde(default)]
    id: String,
}

fn desktop_access_allowed(state: &ServerState) -> bool {
    state.prefs.get_user_or("desktop_access", true)
}

/// The decrypted value is handed straight to an outbound client, so accept only
/// the two schemes a peer file URL can legitimately use.
fn target_url(plaintext: &str) -> anyhow::Result<&str> {
    let trimmed = plaintext.trim();
    anyhow::ensure!(
        trimmed.starts_with("http://") || trimmed.starts_with("https://"),
        "Invalid peer URL"
    );
    let authority = trimmed
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or_default()
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    anyhow::ensure!(!authority.is_empty(), "Invalid peer URL");
    Ok(trimmed)
}

fn decode_target(state: &ServerState, id: &str) -> anyhow::Result<String> {
    let token = state.prefs.get::<String>("url_token").unwrap_or_default();
    let key = crate::utils::base64::base64_decode(token.as_deref().unwrap_or_default());
    anyhow::ensure!(key.len() == 32, "url token is not available");
    let blob = crate::utils::base64::base64_decode(id);
    let plaintext = crate::crypto::xchacha_decrypt_raw(&key, &blob)
        .ok_or_else(|| anyhow::anyhow!("File is expired or does not exist."))?;
    let url = String::from_utf8(plaintext)?;
    Ok(target_url(&url)?.to_owned())
}

/// Hop-by-hop headers must not be copied through a proxy, and the upstream
/// `content-encoding`/`content-length` do not describe the body we re-stream.
const SKIPPED: &[&str] = &[
    "connection",
    "transfer-encoding",
    "upgrade",
    "content-encoding",
    "content-length",
];

pub(super) async fn call(
    State(state): State<ServerState>,
    _remote: ConnectInfo<SocketAddr>,
    Query(params): Query<Params>,
    headers: HeaderMap,
) -> Response {
    if !desktop_access_allowed(&state) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if params.id.is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let url = match decode_target(&state, &params.id) {
        Ok(url) => url,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                format!("File is expired or does not exist. {error}"),
            )
                .into_response();
        }
    };
    proxy(&url, &headers).await
}
pub(super) async fn proxy(url: &str, headers: &HeaderMap) -> Response {
    let client = match reqwest::Client::builder().build() {
        Ok(client) => client,
        Err(error) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
        }
    };
    let mut request = client.get(url);
    if let Some(agent) = headers.get(header::USER_AGENT) {
        request = request.header(header::USER_AGENT, agent);
    }
    let upstream = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
        }
    };
    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut response = Response::builder().status(status);
    for (name, value) in upstream.headers() {
        let name = name.as_str();
        if SKIPPED.contains(&name) {
            continue;
        }
        if let Ok(name) = HeaderName::from_bytes(name.as_bytes()) {
            response = response.header(name, value.clone());
        }
    }
    let stream = upstream
        .bytes_stream()
        .map(|chunk| chunk.map_err(std::io::Error::other));
    match response.body(Body::from_stream(stream)) {
        Ok(response) => response,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/public_proxy.rs"]
mod tests;
