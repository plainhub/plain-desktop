use crate::assembly::server::ServerState;
use crate::host::bridge::Host;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use plain_rs::files::filesystem::browse::Request;
use plain_rs::files::filesystem::record::FileRecord;
use serde_json::{Value, json};
pub(crate) async fn read(
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
/// [`plain_rs::files::filesystem`] walk produces the page. `Request` doubles as the
/// app's `files/read` body, so the public `files` root reuses it verbatim.
pub(crate) async fn execute(host: &Host, request: Request) -> Result<Value, String> {
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
    crate::services::files::file_access::authorize_read(host, path).await
}

/// A single path stat behind the same authorization receipt. `None` covers
/// every "cannot tell" case — missing, unreadable, outside the granted
/// roots — so callers can treat it as a total predicate.
pub(crate) async fn stat_record(host: &Host, path: &str) -> Option<FileRecord> {
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
#[path = "../../../tests/unit/http/routes/file_reads.rs"]
mod tests;
