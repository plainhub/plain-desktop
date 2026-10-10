use crate::assembly::server::ServerState;
use crate::services::files::file_access::receipt;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use plain_rs::files::filesystem::writes::Operation;
use serde_json::{Value, json};

pub(crate) async fn write(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(operation): Json<Operation>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match execute(&state, operation).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, Json(json!({"error":error}))).into_response(),
    }
}
async fn execute(state: &ServerState, operation: Operation) -> Result<Value, String> {
    let path = operation.path().to_owned();
    if !std::path::Path::new(&path).is_absolute() {
        return Err("absolute file path required".into());
    }
    receipt(state, "fileTaskAuthorize", &path).await?;
    tokio::task::spawn_blocking(move || operation.apply())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    receipt(state, "fileTaskScan", &path).await?;
    let entry = plain_rs::files::filesystem::stat(std::path::Path::new(&path))
        .await
        .map_err(|e| e.to_string())?;
    let mut record = plain_rs::files::filesystem::record::FileRecord::from(entry);
    record.permission = "rw".into();
    serde_json::to_value(record).map_err(|e| e.to_string())
}

#[cfg(test)]
#[path = "../../../tests/unit/http/routes/file_writes.rs"]
mod tests;
