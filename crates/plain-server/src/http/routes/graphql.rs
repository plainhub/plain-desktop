use crate::assembly::server::ServerState;
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

/// Answer a development request: the literal client id `dev`, plaintext in
/// and out. Mirrors the NAS dev branch the legacy router provided, so the
/// NAS keeps this entry point after moving to the public router.
async fn run_development(state: &ServerState, body: Bytes) -> Result<Response, StatusCode> {
    let request = String::from_utf8(body.to_vec()).map_err(|_| StatusCode::BAD_REQUEST)?;
    let result = run_with_context(
        &state.public,
        &request,
        crate::graphql::executor::RequestContext::authenticated_remote(
            state.clone(),
            "dev".to_owned(),
        ),
    )
    .await?;
    Ok(result.into_response())
}

/// Does this `Authorization` header present the NAS development token?
/// An unset token never matches, and the comparison is constant time so a
/// wrong guess cannot learn the real one from response timing.
pub(crate) fn dev_bearer_matches(state: &ServerState, authorization: &str) -> Result<bool, ()> {
    let expected = plain_rs::prefs::system::dev_token(&state.prefs).map_err(|_| ())?;
    if expected.is_empty() {
        return Ok(false);
    }
    Ok(authorization
        .strip_prefix("Bearer ")
        .is_some_and(|token| token.as_bytes().ct_eq(expected.as_bytes()).into()))
}

/// Answer the core's own shell. There is no session and therefore no session
/// key, so the body travels in plaintext on the loopback listener.
async fn run_local(state: &ServerState, body: Bytes) -> Result<Response, StatusCode> {
    let request = String::from_utf8(body.to_vec()).map_err(|_| StatusCode::BAD_REQUEST)?;
    let client_id = state
        .native_client_id()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let result = run_with_context(
        &state.public,
        &request,
        crate::graphql::executor::RequestContext::authenticated_local(state.clone(), client_id),
    )
    .await?;
    Ok(result.into_response())
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
    if !state.prefs.get_user_or("desktop_access", true) {
        return Err(StatusCode::NOT_FOUND);
    }
    // The NAS development bearer. It stands in for a session so the NAS
    // can be driven before setup, and it answers in plaintext for the same
    // reason — no session key to encrypt with.
    let dev_bearer = match authorization {
        Some(value) => {
            dev_bearer_matches(state, value).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        }
        None => false,
    };
    if dev_bearer && !client_id.is_empty() {
        return run_development(&state, body).await;
    }

    // The shell talks to its own core as itself: its `c-id` is the core's
    // own client id, and no login ever created a session for it. Remote
    // callers still need one.
    if client_id == state.native_client_id().unwrap_or_default() {
        return run_local(&state, body).await;
    }

    if client_id.is_empty() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let session = state
        .db
        .session_get(client_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let (token_mode, key) = if authorization.is_none() {
        let key = crate::http::sessions::host::key(&state.db, client_id)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;
        (true, key)
    } else {
        if session.r#type != "CUSTOM"
            || !valid_custom_bearer(authorization.unwrap(), &session.token)
        {
            return Err(StatusCode::UNAUTHORIZED);
        }
        (false, Vec::new())
    };

    let request = if token_mode {
        let decrypted =
            plain_rs::crypto::xchacha_decrypt_raw(&key, &body).ok_or(StatusCode::UNAUTHORIZED)?;
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

    crate::http::sessions::host::touch(state, client_id)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let result = run_with_context(
        &state.public,
        &request,
        crate::graphql::executor::RequestContext::authenticated_remote(
            state.clone(),
            client_id.to_owned(),
        ),
    )
    .await?;
    let mut response = if token_mode {
        let encrypted = plain_rs::crypto::xchacha_encrypt_raw(&key, &result)
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
pub(crate) async fn run(
    schema: &crate::graphql::schemas::public::PublicSchema,
    request: &str,
    state: Option<ServerState>,
) -> Result<Vec<u8>, StatusCode> {
    run_with_context(
        schema,
        request,
        crate::graphql::executor::RequestContext::remote(state),
    )
    .await
}

async fn run_with_context(
    schema: &crate::graphql::schemas::public::PublicSchema,
    request: &str,
    context: crate::graphql::executor::RequestContext,
) -> Result<Vec<u8>, StatusCode> {
    let parsed: async_graphql::Request =
        serde_json::from_str(request).map_err(|_| StatusCode::BAD_REQUEST)?;
    serde_json::to_vec(&crate::graphql::executor::execute_graphql(schema, parsed, context).await)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

pub(crate) async fn call(
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
#[path = "../../../tests/unit/http/routes/graphql.rs"]
mod tests;

pub(crate) async fn health() -> &'static str {
    "OK"
}

fn shutdown_allowed(address: SocketAddr) -> bool {
    matches!(address.ip(), std::net::IpAddr::V4(ip) if ip == std::net::Ipv4Addr::LOCALHOST)
        || matches!(address.ip(), std::net::IpAddr::V6(ip) if ip == std::net::Ipv6Addr::LOCALHOST)
}

pub(crate) async fn shutdown(
    State(state): State<ServerState>,
    ConnectInfo(address): ConnectInfo<SocketAddr>,
) -> Response {
    if !shutdown_allowed(address) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match crate::graphql::resolvers::settings::state::apply_user_patch(
        crate::graphql::resolvers::settings::state::UserPatchEnv::from_server(&state),
        plain_rs::prefs::user::UserSettingsPatch {
            desktop_access: Some(false),
            ..Default::default()
        },
    )
    .await
    {
        Ok(_) => {}
        Err(error) => {
            log::error!("Desktop access shutdown failed: {error:?}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }
    StatusCode::GONE.into_response()
}

pub(crate) async fn init(
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
    if !state.prefs.get_user_or("desktop_access", true) {
        return StatusCode::NOT_FOUND.into_response();
    }
    // The shell decrypts `/init` with the local URL token, like every other
    // local request.
    let key = crate::http::sessions::host::local_key(&state, client_id).or(
        crate::http::sessions::host::key(&state.db, client_id)
            .ok()
            .flatten(),
    );
    let authenticated = key
        .as_ref()
        .is_some_and(|key| plain_rs::crypto::xchacha_decrypt_raw(key, &body).is_some());
    let password = if !authenticated
        && state
            .prefs
            .get_or(plain_rs::prefs::system::keys::PASSWORD_TYPE, 2_i32)
            == 2
    {
        match crate::http::auth::ws_login::reset_password(&state) {
            Ok(password) => password,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    } else {
        String::new()
    };
    let pair = match crate::services::peers::peer_wire::signing_keypair(&state.prefs) {
        Ok(pair) => pair,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let _ = remote;
    Json(json!({"signaturePublicKey":plain_rs::utils::base64::base64_encode(&pair[32..]),"password":password}))
        .into_response()
}
