use crate::assembly::server::ServerState;
use anyhow::Result;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use plain_rs::chat::messages::message_commands;
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum Request {
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
        attachments: Vec<plain_rs::chat::messages::share_send::Attachment>,
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
use crate::services::chat::chat_actions::committed;
pub(crate) async fn call(
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
                crate::services::chat::chat_actions::retry(&state, id).await?,
            )?),
            Request::Items {
                target,
                offset,
                limit,
                query,
            } => {
                let (peer, channel) = message_commands::target(&target)?;
                let text = plain_rs::utils::search_dsl::parse(&query)
                    .into_iter()
                    .find(|f| f.name == "text")
                    .map(|f| f.value.trim().to_string())
                    .unwrap_or_default();
                let mut value = plain_rs::db::chat_store::messages::list(
                    &state.db,
                    &plain_rs::db::chat_store::messages::Filter {
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
                crate::services::chat::chat_actions::send(&state, &target, &content)
                    .await?,
            )?),
            Request::SendText { targets, text } => Ok(serde_json::to_value(
                crate::services::chat::chat_actions::send_text(&state, targets, &text)
                    .await?,
            )?),
            Request::SendMany { targets, content } => Ok(serde_json::to_value(
                crate::services::chat::chat_actions::send_many(&state, targets, content)
                    .await?,
            )?),
            Request::Share {
                targets,
                attachments,
                text,
                caption,
                images,
                normalize,
            } => {
                crate::services::chat::chat_actions::share(
                    &state, targets, attachments, text, caption, images, normalize,
                )
                .await
            }
            Request::ShareContent { id } => {
                crate::services::chat::chat_actions::share_content(&state, &id).await
            }
            Request::Folder {
                targets,
                path,
                name,
                expires_at,
            } => {
                if path.trim().is_empty() {
                    return Ok(json!({"ok":false,"chats":[]}));
                }
                let service = plain_rs::files::shares::Service::new(state.db.clone(), state.prefs.clone());
                let row = service.create(name, vec![path], true, expires_at)?;
                let content =
                    crate::services::chat::chat_actions::share_content(&state, &row.id)
                        .await?
                        .to_string();
                let rows = crate::services::chat::chat_actions::send_many(
                    &state, targets, content,
                )
                .await?;
                Ok(json!({"ok":true,"chats":rows}))
            }
            Request::Forward { id, target } => {
                let row = plain_rs::db::chat_store::messages::get(&state.db, &id)?
                    .ok_or_else(|| anyhow::anyhow!("Message unavailable"))?;
                Ok(serde_json::to_value(
                    crate::services::chat::chat_actions::send(
                        &state,
                        &target,
                        &row.content,
                    )
                    .await?,
                )?)
            }
            Request::Delete { ids } => Ok(json!(
                crate::services::chat::chat_actions::delete(&state, ids)?
            )),
            Request::DeleteQuery { query } => Ok(json!(
                crate::services::chat::chat_actions::delete_query(&state, &query)?
            )),
            Request::Create { target, content } => {
                let db = state.db.clone();
                let chat = tokio::task::spawn_blocking(move || {
                    let (to, channel) = message_commands::target(&target)?;
                    plain_rs::chat::messages::message_lifecycle::create(&db, &to, &channel, &content)
                })
                .await??;
                committed(&state, &chat, plain_rs::chat::events::WS_MESSAGE_CREATED);
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
                committed(&state, &chat, plain_rs::chat::events::WS_MESSAGE_CREATED);
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
                committed(&state, &chat, plain_rs::chat::events::WS_MESSAGE_UPDATED);
                if chat.channel_id.is_empty() && (chat.to_id.is_empty() || chat.to_id == "local") {
                    return Ok(serde_json::to_value(chat)?);
                }
                Ok(serde_json::to_value(
                    crate::services::chat::chat_delivery::deliver(&state, &id, None)
                        .await?
                        .chat,
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
                        committed(&state, &chat, plain_rs::chat::events::WS_MESSAGE_UPDATED);
                        if chat.channel_id.is_empty()
                            && (chat.to_id.is_empty() || chat.to_id == "local")
                        {
                            return Ok(Some(chat));
                        }
                        Ok(crate::services::chat::chat_delivery::deliver(
                            &state, &chat.id, None,
                        )
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
                    plain_rs::chat::attachments::app_file_store::chat_deletion::delete(
                        &db,
                        &directory,
                        if channel.is_empty() {
                            plain_rs::chat::attachments::app_file_store::chat_deletion::Selection::Peer(&peer)
                        } else {
                            plain_rs::chat::attachments::app_file_store::chat_deletion::Selection::Channel(
                                &channel,
                            )
                        },
                    )
                })
                .await??;
                let _ = state.events.send(plain_rs::events::WsEvent::broadcast(
                    plain_rs::chat::events::WS_MESSAGE_DELETED,
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
#[path = "../../../tests/unit/services/chat/chat_service.rs"]
mod tests;
