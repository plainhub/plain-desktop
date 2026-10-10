use super::server::ServerState;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Request {
    Delete {
        ids: Vec<String>,
    },
    Reply {
        id: String,
        #[serde(rename = "actionIndex")]
        action_index: i32,
        text: String,
    },
}
fn validate_receipt(ids: &[String], receipt: Value) -> anyhow::Result<usize> {
    let completed: Vec<String> = serde_json::from_value(receipt)?;
    let completed: std::collections::HashSet<_> = completed.into_iter().collect();
    anyhow::ensure!(
        completed.iter().all(|id| ids.contains(id)),
        "invalid notification cancellation receipt"
    );
    Ok(completed.len())
}
async fn execute(state: &ServerState, request: Request) -> anyhow::Result<Value> {
    Ok(match request {
        Request::Delete { ids } => {
            let receipt = state
                .host
                .call("systemCancelNotifications", json!({"ids":ids}))
                .await
                .map_err(anyhow::Error::msg)?;
            json!({"count":validate_receipt(&ids,receipt)?})
        }
        Request::Reply {
            id,
            action_index,
            text,
        } => {
            anyhow::ensure!(action_index >= 0, "action_not_found");
            let receipt = state
                .host
                .call(
                    "systemReplyNotification",
                    json!({"id":id,"actionIndex":action_index,"text":text}),
                )
                .await
                .map_err(anyhow::Error::msg)?;
            let ok = receipt
                .as_bool()
                .ok_or_else(|| anyhow::anyhow!("invalid notification reply receipt"))?;
            json!({"ok":ok})
        }
    })
}
pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match execute(&state, request).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":error.to_string()})),
        )
            .into_response(),
    }
}
#[cfg(test)]
#[path = "../../tests/unit/content_api/notification_actions.rs"]
mod tests;
