use super::server::ServerState;
use axum::{
    body::Bytes,
    extract::{ConnectInfo, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::net::SocketAddr;

pub(super) async fn call(
    State(state): State<ServerState>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    body: Bytes,
) -> Response {
    let body = match std::str::from_utf8(&body) {
        Ok(body) => body,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid UTF-8").into_response(),
    };
    match super::pairing_runtime::handle_nearby_post(&state, body, &remote.ip().to_string()).await {
        Ok(true) => (StatusCode::OK, "1").into_response(),
        Ok(false) => (StatusCode::BAD_REQUEST, "unknown message type").into_response(),
        Err(error) => {
            log::debug!("nearby request rejected: {error}");
            (StatusCode::BAD_REQUEST, "invalid nearby message").into_response()
        }
    }
}
