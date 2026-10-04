use super::server::ServerState;
use crate::chat::nearby_http::{self, Message};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Request {
    Send {
        ip: String,
        port: u16,
        message: Message,
    },
    Probe {
        ip: String,
        port: u16,
    },
}
pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let result = match request {
        Request::Send { ip, port, message } => nearby_http::send(&ip, port, &message).await,
        Request::Probe { ip, port } => nearby_http::probe(&ip, port).await,
    };
    match result {
        Ok(value) => Json(json!({"result":value})).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":error.to_string()})),
        )
            .into_response(),
    }
}
