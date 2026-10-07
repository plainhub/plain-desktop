use super::server::ServerState;
use axum::{
    Json,
    body::Bytes,
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::net::SocketAddr;
use subtle::ConstantTimeEq;

fn bearer_value(value: &str) -> &str {
    value.split(' ').nth(1).unwrap_or("")
}

fn valid_custom_bearer(value: &str, session_token: &str) -> bool {
    let bearer = bearer_value(value);
    !bearer.is_empty()
        && !session_token.is_empty()
        && bearer
            .as_bytes()
            .ct_eq(session_token.as_bytes())
            .unwrap_u8()
            == 1
}

async fn execute(
    state: &ServerState,
    headers: &HeaderMap,
    body: Bytes,
) -> Result<Response, StatusCode> {
    let client_id = headers
        .get("c-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let authorization = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok());
    let facts = state
        .host
        .call("mainGraphqlAuthFacts", json!({"clientId":client_id}))
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    if facts["desktopAccessEnabled"].as_bool() != Some(true) {
        return Err(StatusCode::NOT_FOUND);
    }
    if client_id.is_empty() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let (token_mode, key) = if authorization.is_none() {
        let token = facts["tokenKey"].as_str().unwrap_or_default();
        let key = crate::utils::base64::base64_decode(token);
        if key.len() != 32 {
            return Err(StatusCode::UNAUTHORIZED);
        }
        (true, key)
    } else {
        let token = facts["customSessionToken"].as_str().unwrap_or_default();
        if !valid_custom_bearer(authorization.unwrap(), token) {
            return Err(StatusCode::UNAUTHORIZED);
        }
        (false, Vec::new())
    };

    let request = if token_mode {
        let decrypted =
            crate::crypto::xchacha_decrypt_raw(&key, &body).ok_or(StatusCode::UNAUTHORIZED)?;
        if decrypted.is_empty() {
            return Err(StatusCode::UNAUTHORIZED);
        }
        let plaintext = String::from_utf8_lossy(&decrypted).into_owned();
        let now = chrono::Utc::now().timestamp_millis();
        state
            .main_replay
            .admit(client_id, &plaintext, now)
            .map_err(|_| StatusCode::BAD_REQUEST)?
            .to_owned()
    } else {
        String::from_utf8(body.to_vec()).map_err(|_| StatusCode::BAD_REQUEST)?
    };

    // The body is the contract's own request envelope, so it parses straight
    // into async-graphql's request type. `client_id` has already done its
    // job by this point — auth, and the replay guard above.
    let result = run(&state.public, &request, Some(state.clone())).await?;
    let mut response = if token_mode {
        let encrypted = crate::crypto::xchacha_encrypt_raw(&key, &result)
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        encrypted.into_response()
    } else {
        result.into_response()
    };
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        CONTENT_TYPE,
        if token_mode {
            "application/octet-stream"
        } else {
            "application/json"
        }
        .parse()
        .unwrap(),
    );
    Ok(response)
}

/// Executes a decrypted request envelope against the contract schema.
///
/// Split out of [`execute`] so the flip itself — Rust answers `/graphql`,
/// nothing is handed to the platform — is testable without standing up a
/// whole server: a document that queries the schema and never reaches the
/// host is the thing to pin down.
pub(super) async fn run(
    schema: &super::public_schema::PublicSchema,
    request: &str,
    state: Option<ServerState>,
) -> Result<Vec<u8>, StatusCode> {
    let parsed: async_graphql::Request =
        serde_json::from_str(request).map_err(|_| StatusCode::BAD_REQUEST)?;
    let parsed = match state {
        Some(state) => parsed.data(state),
        None => parsed,
    };
    serde_json::to_vec(&schema.execute(parsed).await).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    match execute(&state, &headers, body).await {
        Ok(response) => response,
        Err(status) => status.into_response(),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/main_graphql.rs"]
mod tests;

pub(super) async fn health(State(state): State<ServerState>) -> Response {
    match state.host.call("mainGraphqlHealth", json!({})).await {
        Ok(value) => value
            .as_str()
            .unwrap_or_default()
            .to_owned()
            .into_response(),
        Err(_) => StatusCode::BAD_GATEWAY.into_response(),
    }
}

fn shutdown_allowed(address: SocketAddr) -> bool {
    matches!(address.ip(), std::net::IpAddr::V4(ip) if ip == std::net::Ipv4Addr::LOCALHOST)
        || matches!(address.ip(), std::net::IpAddr::V6(ip) if ip == std::net::Ipv6Addr::LOCALHOST)
}

pub(super) async fn shutdown(
    State(state): State<ServerState>,
    ConnectInfo(address): ConnectInfo<SocketAddr>,
) -> Response {
    if !shutdown_allowed(address) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let _ = state.host.call("mainGraphqlShutdown", json!({})).await;
    StatusCode::GONE.into_response()
}

pub(super) async fn init(
    State(state): State<ServerState>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let client_id = headers
        .get("c-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if client_id.is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let facts = match state
        .host
        .call("mainGraphqlInitFacts", json!({"clientId":client_id}))
        .await
    {
        Ok(facts) => facts,
        Err(_) => return StatusCode::BAD_GATEWAY.into_response(),
    };
    if facts["desktopAccessEnabled"].as_bool() != Some(true) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let token = facts["tokenKey"].as_str().unwrap_or_default();
    let key = crate::utils::base64::base64_decode(token);
    let token_authenticated =
        key.len() == 32 && crate::crypto::xchacha_decrypt_raw(&key, &body).is_some();
    let response = match state
        .host
        .call(
            "mainGraphqlInitResponse",
            json!({
                "clientId": client_id,
                "remoteHost": remote.ip().to_string(),
                "resetPassword": !token_authenticated,
            }),
        )
        .await
    {
        Ok(response) => response,
        Err(_) => return StatusCode::BAD_GATEWAY.into_response(),
    };
    Json(response).into_response()
}
