use super::server::ServerState;
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
};
use serde_json::json;
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
            crate::crypto::chacha20_decrypt(&key, &body).ok_or(StatusCode::UNAUTHORIZED)?;
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

    let result = state
        .host
        .call(
            "mainGraphqlExecute",
            json!({"clientId":client_id,"request":request}),
        )
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    let result = result
        .as_str()
        .ok_or(StatusCode::BAD_GATEWAY)?
        .as_bytes()
        .to_vec();
    let mut response = if token_mode {
        let encrypted = crate::crypto::chacha20_encrypt(&key, &result)
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
