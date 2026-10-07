use super::{channel_delivery, server::ServerState};
use crate::{
    chat::{
        app_file_store::chat_deletion,
        channel::state::{self, Action},
        enums::ChannelSystemMessageType,
    },
    db::{
        DChannel,
        chat_store::{channels, peers},
    },
};
use anyhow::{Result, ensure};
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
    Snapshot,
    Create {
        name: String,
    },
    Rename {
        id: String,
        name: String,
    },
    Delete {
        id: String,
    },
    Leave {
        id: String,
    },
    Invite {
        id: String,
        peer: String,
    },
    Resend {
        id: String,
        peer: String,
    },
    Kick {
        id: String,
        peer: String,
    },
    Accept {
        id: String,
    },
    Decline {
        id: String,
    },
    Send {
        id: String,
        message_type: ChannelSystemMessageType,
        target: String,
    },
}
fn with_channels(state: &ServerState, mut result: Value) -> Result<Value> {
    result["channels"] = crate::chat::events::public_channels(&state.db)?;
    Ok(result)
}
fn snapshot(state: &ServerState) -> Result<Value> {
    with_channels(
        state,
        json!({"accepted":true,"changed":true,"broadcast":false,"channel":null,"invite":null,"cancel":null}),
    )
}
fn emit(state: &ServerState, result: Value) {
    let _ = state.events.send(crate::ws_event::WsEvent::broadcast(
        crate::chat::events::WS_CHANNELS_UPDATED,
        result.to_string(),
    ));
}
pub(super) fn publish(state: &ServerState, result: Value) -> Result<()> {
    emit(state, with_channels(state, result)?);
    Ok(())
}
fn notify_changed(state: &ServerState) -> Result<()> {
    emit(state, snapshot(state)?);
    Ok(())
}
pub(super) async fn receive(
    state: &ServerState,
    from: &str,
    kind: ChannelSystemMessageType,
    payload: &str,
) -> Result<Value> {
    let actor = state
        .prefs
        .get::<String>("client_id")?
        .filter(|id| !id.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Missing client identity"))?;
    let received = crate::chat::channel::incoming::receive(&state.db, &actor, from, kind, payload)?;
    let result = serde_json::to_value(&received)?;
    if received.changed || received.invite.is_some() || received.cancel.is_some() {
        publish(state, result.clone())?;
    }
    if received.broadcast {
        if let Some(channel) = received.channel {
            let state = state.clone();
            tokio::spawn(async move {
                if let Err(error) = super::channel_delivery::send(
                    &state,
                    &channel,
                    ChannelSystemMessageType::Update,
                    "",
                    false,
                )
                .await
                {
                    log::warn!("Channel update broadcast: {error}");
                }
            });
        }
    }
    Ok(result)
}
fn get(state: &ServerState, id: &str) -> Result<DChannel> {
    channels::get(&state.db, id)?.ok_or_else(|| anyhow::anyhow!("Channel not found"))
}
async fn send(
    state: &ServerState,
    channel: &DChannel,
    kind: ChannelSystemMessageType,
    target: &str,
    removed: bool,
) -> Value {
    channel_delivery::send(state, channel, kind, target, removed)
        .await
        .unwrap_or_else(channel_delivery::rejected)
}
pub(super) async fn execute(state: &ServerState, request: Request) -> Result<Value> {
    if matches!(&request, Request::Snapshot) {
        return snapshot(state);
    }
    let actor = state
        .prefs
        .get::<String>("client_id")?
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Missing client identity"))?;
    let (channel, response) = match request {
        Request::Create { name } => {
            let channel = state::create(&state.db, &actor, &name)?;
            notify_changed(state)?;
            (Some(channel), Value::Null)
        }
        Request::Delete { id } => {
            let channel = chat_deletion::remove_channel(&state.db, &state.directory, &id)?
                .ok_or_else(|| anyhow::anyhow!("Channel not found"))?;
            notify_changed(state)?;
            if channel.owner_id == actor {
                send(state, &channel, ChannelSystemMessageType::Kick, "", true).await;
            }
            (None, Value::Null)
        }
        Request::Decline { id } => {
            let channel = get(state, &id)?;
            ensure!(
                peers::get(&state.db, &channel.owner_id)?.is_some(),
                "Owner peer not found"
            );
            send(
                state,
                &channel,
                ChannelSystemMessageType::InviteDecline,
                &channel.owner_id,
                false,
            )
            .await;
            chat_deletion::remove_channel_if_matches(&state.db, &state.directory, &channel)?;
            notify_changed(state)?;
            (None, Value::Null)
        }
        Request::Send {
            id,
            message_type,
            target,
        } => {
            let channel = get(state, &id)?;
            let response = send(state, &channel, message_type, &target, false).await;
            (channels::get(&state.db, &id)?, response)
        }
        request => {
            let (id, action, kind, target, broadcast, changed) = match request {
                Request::Rename { id, name } => {
                    (id, Action::Rename { name }, None, String::new(), true, true)
                }
                Request::Leave { id } => (
                    id,
                    Action::Leave,
                    Some(ChannelSystemMessageType::Leave),
                    String::new(),
                    false,
                    true,
                ),
                Request::Invite { id, peer } => (
                    id,
                    Action::Invite { peer: peer.clone() },
                    Some(ChannelSystemMessageType::Invite),
                    peer,
                    false,
                    true,
                ),
                Request::Resend { id, peer } => (
                    id,
                    Action::Resend { peer: peer.clone() },
                    Some(ChannelSystemMessageType::Invite),
                    peer,
                    false,
                    false,
                ),
                Request::Kick { id, peer } => (
                    id,
                    Action::Kick { peer: peer.clone() },
                    Some(ChannelSystemMessageType::Kick),
                    peer,
                    true,
                    true,
                ),
                Request::Accept { id } => (
                    id,
                    Action::Accept,
                    Some(ChannelSystemMessageType::InviteAccept),
                    String::new(),
                    false,
                    false,
                ),
                _ => unreachable!(),
            };
            let channel = state::apply(&state.db, &actor, &id, action)?;
            if changed {
                notify_changed(state)?;
            }
            let target = if target.is_empty() {
                channel.owner_id.clone()
            } else {
                target
            };
            let response = if let Some(kind) = kind {
                send(state, &channel, kind, &target, false).await
            } else {
                Value::Null
            };
            if broadcast && channel.owner_id == actor {
                send(state, &channel, ChannelSystemMessageType::Update, "", false).await;
            }
            (channels::get(&state.db, &id)?, response)
        }
    };
    Ok(json!({"channel":channel,"response":response}))
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
        Ok(value) => Json(json!({"result":value})).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":error.to_string()})),
        )
            .into_response(),
    }
}
#[cfg(test)]
#[path = "../../tests/unit/content_api/channel_runtime.rs"]
mod tests;
