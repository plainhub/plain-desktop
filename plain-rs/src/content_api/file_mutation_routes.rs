use super::server::ServerState;
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
    RecoverDeletions,
    Rename {
        path: String,
        name: String,
    },
    Delete {
        path: String,
    },
    Recover {
        #[serde(rename = "clientId")]
        client_id: String,
        id: String,
    },
}
pub(super) async fn mutate(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let result = match request {
        Request::RecoverDeletions => state
            .files
            .recover_deletions()
            .await
            .map(|count| json!({"count":count})),
        Request::Recover { client_id, id } => {
            state.files.recover(client_id, id).await.map(|task| {
                task.map(|task| json!({"id": task.id, "status": task.status}))
                    .unwrap_or(serde_json::Value::Null)
            })
        }
        Request::Rename { path, name } => state
            .files
            .rename(path, name)
            .await
            .map(|path| json!({"path":path})),
        Request::Delete { path } => state
            .files
            .delete(path)
            .await
            .and_then(|value| serde_json::to_value(value).map_err(Into::into)),
    };
    match result {
        Ok(value) => Json(value).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":error.to_string()})),
        )
            .into_response(),
    }
}
#[cfg(test)]
#[path = "../../tests/unit/content_api/file_mutation_routes.rs"]
mod tests;
