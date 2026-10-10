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
    Edit { id: String, text: String },
    Refresh { id: String },
}
pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<serde_json::Value> {
        Ok(match request {
            Request::Edit { id, text } => {
                let result = crate::link_preview::edit(&state.db, &state.directory, &id, &text)?;
                state.previews.request(&id);
                if result.changed {
                    let _ = state.events.send(crate::ws_event::WsEvent::broadcast(
                        crate::chat::events::WS_MESSAGE_UPDATED,
                        json!([crate::chat::service::chat_to_json(&result.chat)]).to_string(),
                    ));
                }
                serde_json::to_value(result)?
            }
            Request::Refresh { id } => json!(state.previews.request(&id)),
        })
    })
    .await;
    match result {Ok(Ok(value))=>Json(json!({"result":value})).into_response(),error=>(StatusCode::BAD_REQUEST,Json(json!({"error":match error {Ok(Err(e))=>e.to_string(),Err(e)=>e.to_string(),_=>unreachable!()}}))).into_response()}
}
