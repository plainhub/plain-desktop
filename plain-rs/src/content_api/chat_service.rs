use super::server::ServerState;
use crate::{chat::message_commands, db::DChat};
use anyhow::Result;
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
    Create {
        target: String,
        content: String,
    },
    CreateFiles {
        target: String,
        items: Vec<Value>,
        images: bool,
    },
    ReplaceFiles {
        id: String,
        items: Vec<Value>,
    },
    ReplaceFilesMany {
        ids: Vec<String>,
        items: Vec<Value>,
    },
    Clear {
        target: String,
    },
}
fn committed(state: &ServerState, chat: &DChat, event: i32) {
    state.previews.request(&chat.id);
    let _ = state.events.send(crate::ws_event::WsEvent::broadcast(
        event,
        json!([crate::chat::service::chat_to_json(chat)]).to_string(),
    ));
}
pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if *state.stop.borrow() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let work: Result<Value> = async {
        match request {
            Request::Create { target, content } => {
                let db = state.db.clone();
                let chat = tokio::task::spawn_blocking(move || {
                    let (to, channel) = message_commands::target(&target)?;
                    crate::chat::message_lifecycle::create(&db, &to, &channel, &content)
                })
                .await??;
                committed(&state, &chat, crate::chat::events::WS_MESSAGE_CREATED);
                Ok(serde_json::to_value(chat)?)
            }
            Request::CreateFiles {
                target,
                items,
                images,
            } => {
                let db = state.db.clone();
                let chat = tokio::task::spawn_blocking(move || {
                    message_commands::create_files(&db, &target, items, images)
                })
                .await??;
                committed(&state, &chat, crate::chat::events::WS_MESSAGE_CREATED);
                Ok(serde_json::to_value(chat)?)
            }
            Request::ReplaceFiles { id, items } => {
                let db = state.db.clone();
                let directory = state.directory.clone();
                let edit_id = id.clone();
                let Some(chat) = tokio::task::spawn_blocking(move || {
                    message_commands::replace_files(&db, &directory, &edit_id, items)
                })
                .await??
                else {
                    return Ok(Value::Null);
                };
                committed(&state, &chat, crate::chat::events::WS_MESSAGE_UPDATED);
                if chat.channel_id.is_empty() && (chat.to_id.is_empty() || chat.to_id == "local") {
                    return Ok(serde_json::to_value(chat)?);
                }
                Ok(serde_json::to_value(
                    super::chat_delivery::deliver(&state, &id, None).await?.chat,
                )?)
            }
            Request::ReplaceFilesMany { ids, items } => {
                let db = state.db.clone();
                let directory = state.directory.clone();
                let rows = tokio::task::spawn_blocking(move || {
                    message_commands::replace_many(&db, &directory, &ids, items)
                })
                .await??;
                let results = futures_util::future::join_all(rows.into_iter().map(|row| async {
                    if let Some(chat) = row {
                        committed(&state, &chat, crate::chat::events::WS_MESSAGE_UPDATED);
                        if chat.channel_id.is_empty()
                            && (chat.to_id.is_empty() || chat.to_id == "local")
                        {
                            return Ok(Some(chat));
                        }
                        Ok(super::chat_delivery::deliver(&state, &chat.id, None)
                            .await?
                            .chat)
                    } else {
                        Ok(None)
                    }
                }))
                .await
                .into_iter()
                .collect::<Result<Vec<_>>>()?;
                Ok(serde_json::to_value(results)?)
            }
            Request::Clear { target } => {
                let db = state.db.clone();
                let directory = state.directory.clone();
                let copy = target.clone();
                let count = tokio::task::spawn_blocking(move || {
                    let (peer, channel) = message_commands::target(&copy)?;
                    crate::chat::app_file_store::chat_deletion::delete(
                        &db,
                        &directory,
                        if channel.is_empty() {
                            crate::chat::app_file_store::chat_deletion::Selection::Peer(&peer)
                        } else {
                            crate::chat::app_file_store::chat_deletion::Selection::Channel(&channel)
                        },
                    )
                })
                .await??;
                let _ = state.events.send(crate::ws_event::WsEvent::broadcast(
                    crate::chat::events::WS_MESSAGE_DELETED,
                    json!(target).to_string(),
                ));
                Ok(json!(count))
            }
        }
    }
    .await;
    match work {
        Ok(value) => Json(json!({"result":value})).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":error.to_string()})),
        )
            .into_response(),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/chat_service.rs"]
mod tests;
