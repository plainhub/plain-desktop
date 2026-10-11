use crate::assembly::server::ServerState;
use crate::services::files::writes::execute;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use plain_rs::files::filesystem::writes::Operation;
use serde_json::json;

pub(crate) async fn write(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(operation): Json<Operation>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match execute(&state.host, operation).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, Json(json!({"error":error}))).into_response(),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/http/routes/file_writes.rs"]
mod tests;
