use super::server::ServerState;
use crate::filesystem::writes::Operation;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

pub(super) async fn write(
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
    let entry = crate::filesystem::stat(std::path::Path::new(&path))
        .await
        .map_err(|e| e.to_string())?;
    Ok(
        json!({"name":std::path::Path::new(&entry.path).file_name().unwrap_or_default().to_string_lossy(), "path":entry.path,"permission":"rw","createdAt":entry.created_at.timestamp_millis(),"updatedAt":entry.updated_at.timestamp_millis(),"size":entry.size,"isDir":entry.is_dir,"childCount":entry.child_count}),
    )
}
async fn receipt(state: &ServerState, method: &str, path: &str) -> Result<(), String> {
    let value = state.host.call(method, json!({"paths":[path]})).await?;
    if value.as_bool() != Some(true) {
        return Err("invalid file host receipt".into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/file_writes.rs"]
mod tests;
