use super::{file_access::receipt, server::ServerState};
use crate::filesystem::{browse::Request, record::FileRecord};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
pub(super) async fn read(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match execute(&state, request).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, Json(json!({"error":error}))).into_response(),
    }
}
async fn execute(state: &ServerState, request: Request) -> Result<Value, String> {
    let plan = request.plan().map_err(|e| e.to_string())?;
    let page = if let Some(archive) = plan.archive_path() {
        receipt(state, "fileTaskAuthorize", archive).await?;
        let rows = state
            .host
            .call("fileTaskZipEntries", json!({"paths":[plan.root]}))
            .await?;
        let entries: Vec<FileRecord> = serde_json::from_value(rows).map_err(|e| e.to_string())?;
        plan.archive_page(entries).map_err(|e| e.to_string())?
    } else {
        receipt(state, "fileTaskAuthorize", &plan.root).await?;
        tokio::task::spawn_blocking(move || plan.execute())
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?
    };
    serde_json::to_value(page).map_err(|e| e.to_string())
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/file_reads.rs"]
mod tests;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StatRequest {
    path: String,
}
pub(super) async fn stat(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<StatRequest>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if !std::path::Path::new(&request.path).is_absolute() {
        return Json(json!({"file":null})).into_response();
    }
    if receipt(&state, "fileTaskAuthorize", &request.path)
        .await
        .is_err()
    {
        return Json(json!({"file":null})).into_response();
    }
    let record = tokio::task::spawn_blocking(move || {
        FileRecord::stat(std::path::Path::new(&request.path)).ok()
    })
    .await;
    match record {
        Ok(file) => Json(json!({"file":file})).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error":error.to_string()})),
        )
            .into_response(),
    }
}
