use super::server::ServerState;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<Value>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(directory) = state.prefs.path().parent() else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "Missing TLS directory"})),
        )
            .into_response();
    };
    let path = directory.join("tls-identity.json");
    let result = tokio::task::spawn_blocking(move || {
        crate::tls_identity::action(&path, &command.to_string())
            .and_then(|body| serde_json::from_str::<Value>(&body).map_err(|e| e.to_string()))
    })
    .await;
    match result {
        Ok(Ok(value)) => Json(value).into_response(),
        Ok(Err(error)) => (StatusCode::BAD_REQUEST, Json(json!({"error": error}))).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": error.to_string()})),
        )
            .into_response(),
    }
}
