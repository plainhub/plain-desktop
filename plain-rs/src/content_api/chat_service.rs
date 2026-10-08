use super::server::ServerState;
use crate::chat::message_commands;
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
    Retry {
        id: String,
    },
    Items {
        target: String,
        offset: i64,
        limit: i64,
        query: String,
    },
    Send {
        target: String,
        content: String,
    },
    SendText {
        targets: Vec<String>,
        text: String,
    },
    SendMany {
        targets: Vec<String>,
        content: String,
    },
    Share {
        targets: Vec<String>,
        uris: Vec<String>,
        text: Option<String>,
        caption: Option<String>,
        images: Option<bool>,
        normalize: bool,
    },
    ShareContent {
        id: String,
    },
    Folder {
        targets: Vec<String>,
        path: String,
        name: String,
        expires_at: Option<String>,
    },
    Forward {
        id: String,
        target: String,
    },
    Delete {
        ids: Vec<String>,
    },
    DeleteQuery {
        query: String,
    },
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
use super::chat_actions::committed;
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
            Request::Retry { id } => Ok(serde_json::to_value(
                super::chat_actions::retry(&state, id).await?,
            )?),
            Request::Items {
                target,
                offset,
                limit,
                query,
            } => {
                let (peer, channel) = message_commands::target(&target)?;
                let text = crate::utils::search_dsl::parse(&query)
                    .into_iter()
                    .find(|f| f.name == "text")
                    .map(|f| f.value.trim().to_string())
                    .unwrap_or_default();
                let mut value = crate::db::chat_store::messages::list(
                    &state.db,
                    &crate::db::chat_store::messages::Filter {
                        peer: channel.is_empty().then_some(peer),
                        channel: (!channel.is_empty()).then_some(channel),
                        text,
                        offset,
                        limit: Some(limit),
                        descending: true,
                        latest: false,
                        count_only: false,
                    },
                )?;
                if let Some(rows) = value.as_array_mut() {
                    rows.reverse();
                }
                Ok(value)
            }
            Request::Send { target, content } => Ok(serde_json::to_value(
                super::chat_actions::send(&state, &target, &content).await?,
            )?),
            Request::SendText { targets, text } => Ok(serde_json::to_value(
                super::chat_actions::send_text(&state, targets, &text).await?,
            )?),
            Request::SendMany { targets, content } => Ok(serde_json::to_value(
                super::chat_actions::send_many(&state, targets, content).await?,
            )?),
            Request::Share {
                targets,
                uris,
                text,
                caption,
                images,
                normalize,
            } => {
                super::chat_actions::share(&state, targets, uris, text, caption, images, normalize)
                    .await
            }
            Request::ShareContent { id } => super::chat_actions::share_content(&state, &id).await,
            Request::Folder {
                targets,
                path,
                name,
                expires_at,
            } => {
                if path.trim().is_empty() {
                    return Ok(json!({"ok":false,"chats":[]}));
                }
                let token = crate::base64_encode(&crate::random_bytes(32));
                let service = crate::shares::Service::new(state.db.clone(), state.prefs.clone());
                let row = service.create(name, vec![path], token, true, expires_at)?;
                let content = super::chat_actions::share_content(&state, &row.id)
                    .await?
                    .to_string();
                let rows = super::chat_actions::send_many(&state, targets, content).await?;
                Ok(json!({"ok":true,"chats":rows}))
            }
            Request::Forward { id, target } => {
                let row = crate::db::chat_store::messages::get(&state.db, &id)?
                    .ok_or_else(|| anyhow::anyhow!("Message unavailable"))?;
                Ok(serde_json::to_value(
                    super::chat_actions::send(&state, &target, &row.content).await?,
                )?)
            }
            Request::Delete { ids } => Ok(json!(super::chat_actions::delete(&state, ids)?)),
            Request::DeleteQuery { query } => {
                Ok(json!(super::chat_actions::delete_query(&state, &query)?))
            }
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
