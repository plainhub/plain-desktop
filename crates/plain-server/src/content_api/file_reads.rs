use super::{host::Host, server::ServerState};
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
    match execute(&state.host, request).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, Json(json!({"error":error}))).into_response(),
    }
}

/// One directory read: the host authorizes the root, then the shared
/// [`crate::filesystem`] walk produces the page. `Request` doubles as the
/// app's `files/read` body, so the public `files` root reuses it verbatim.
pub(super) async fn execute(host: &Host, request: Request) -> Result<Value, String> {
    let plan = request.plan().map_err(|e| e.to_string())?;
    let page = if let Some(archive) = plan.archive_path() {
        authorize(host, archive).await?;
        let rows = host
            .call("fileTaskZipEntries", json!({"paths":[plan.root]}))
            .await?;
        let entries: Vec<FileRecord> = serde_json::from_value(rows).map_err(|e| e.to_string())?;
        plan.archive_page(entries).map_err(|e| e.to_string())?
    } else {
        authorize(host, &plan.root).await?;
        tokio::task::spawn_blocking(move || plan.execute())
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?
    };
    serde_json::to_value(page).map_err(|e| e.to_string())
}

async fn authorize(host: &Host, path: &str) -> Result<(), String> {
    let value = host
        .call("fileTaskAuthorize", json!({"paths":[path]}))
        .await?;
    if value.as_bool() == Some(true) {
        Ok(())
    } else {
        Err("invalid file host receipt".into())
    }
}

/// A single path stat behind the same authorization receipt. `None` covers
/// every "cannot tell" case — missing, unreadable, outside the granted
/// roots — so callers can treat it as a total predicate.
pub(super) async fn stat_record(host: &Host, path: &str) -> Option<FileRecord> {
    if !std::path::Path::new(path).is_absolute() {
        return None;
    }
    if authorize(host, path).await.is_err() {
        return None;
    }
    let owned = path.to_string();
    tokio::task::spawn_blocking(move || FileRecord::stat(std::path::Path::new(&owned)).ok())
        .await
        .ok()
        .flatten()
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
    match stat_record(&state.host, &request.path).await {
        Some(file) => Json(json!({"file":file})).into_response(),
        None => Json(json!({"file":null})).into_response(),
    }
}
