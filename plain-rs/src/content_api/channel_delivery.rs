use super::server::ServerState;
use crate::{
    chat::{
        channel::messages::ChannelMember,
        enums::{ChannelSystemMessageType, DeviceType},
    },
    db::{
        DChannel,
        chat_store::{channels, peers},
    },
};
use anyhow::{Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};

pub(super) fn rejected(message: impl ToString) -> Value {
    json!({"errors":[{"message":message.to_string()}]})
}
fn current(state: &ServerState, channel: &DChannel, removed: bool) -> Result<bool> {
    let row = channels::get(&state.db, &channel.id)?;
    Ok(if removed {
        row.is_none()
    } else {
        row.as_ref() == Some(channel)
    })
}
pub(super) async fn send(
    state: &ServerState,
    channel: &DChannel,
    kind: ChannelSystemMessageType,
    target: &str,
    removed: bool,
) -> Result<Value> {
    let mut stop = state.stop.clone();
    ensure!(!*stop.borrow(), "Server stopped");
    tokio::select! {
        biased;
        _ = stop.changed() => Ok(rejected("Server stopped")),
        result = deliver(state, channel, kind, target, removed) => result,
    }
}
async fn deliver(
    state: &ServerState,
    channel: &DChannel,
    kind: ChannelSystemMessageType,
    target: &str,
    removed: bool,
) -> Result<Value> {
    ensure!(
        current(state, channel, removed)?,
        "Channel changed before send"
    );
    let actor = state.prefs.get::<String>("client_id")?.unwrap_or_default();
    ensure!(!actor.is_empty(), "Missing client identity");
    match kind {
        ChannelSystemMessageType::Invite
        | ChannelSystemMessageType::Update
        | ChannelSystemMessageType::Kick => ensure!(
            channel.owner_id == actor,
            "Only owner can send channel updates"
        ),
        _ => ensure!(
            !target.is_empty() && target == channel.owner_id,
            "Invalid channel owner destination"
        ),
    }
    ensure!(
        kind != ChannelSystemMessageType::Invite || !target.is_empty(),
        "Missing invite target"
    );
    let ids = if target.is_empty() {
        serde_json::from_str::<Vec<ChannelMember>>(&channel.members)?
            .into_iter()
            .filter(|m| m.peer_id != actor)
            .map(|m| m.peer_id)
            .collect::<Vec<_>>()
    } else {
        vec![target.to_owned()]
    };
    let mut present = false;
    for id in &ids {
        present |= peers::get(&state.db, id)?.is_some();
    }
    if !present {
        return Ok(if target.is_empty() {
            json!({"data":{"channelSystemMessage":true}})
        } else {
            rejected("Peer not found")
        });
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Device {
        name: String,
        device_type: DeviceType,
    }
    let device = if matches!(
        kind,
        ChannelSystemMessageType::Invite
            | ChannelSystemMessageType::Update
            | ChannelSystemMessageType::InviteAccept
    ) {
        serde_json::from_value(
            state
                .host
                .call("peerDeviceInfo", json!({}))
                .await
                .map_err(anyhow::Error::msg)?,
        )?
    } else {
        Device {
            name: String::new(),
            device_type: DeviceType::Unknown,
        }
    };
    ensure!(
        current(state, channel, removed)?,
        "Channel changed before send"
    );
    let prepared = super::channel_outgoing::prepare(
        &state.db,
        &state.prefs,
        channel,
        kind,
        target,
        &device.name,
        device.device_type,
    )?;
    let wire = super::channel_outgoing::wire(&state.prefs, kind, &prepared.payload)?;
    let mut response = json!({"data":{"channelSystemMessage":true}});
    for group in prepared.targets.chunks(4) {
        let results = futures_util::future::join_all(group.iter().map(|target| async {
            let _permit = state
                .channel_delivery
                .acquire()
                .await
                .map_err(|e| anyhow::anyhow!(e))?;
            ensure!(
                current(state, channel, removed)?,
                "Channel changed before send"
            );
            let peer = peers::get(&state.db, &target.peer.id)?
                .ok_or_else(|| anyhow::anyhow!("Peer not found"))?;
            let (key, cid) = if peer.key.is_empty() {
                (&channel.key, channel.id.as_str())
            } else {
                (&peer.key, "")
            };
            let response = super::peer_transport::send_checked(
                &state.host,
                &state.transport,
                &peer,
                cid,
                &crate::base64_decode(key),
                &wire,
                || {
                    if !current(state, channel, removed).map_err(|e| e.to_string())? {
                        return Err("Channel changed before send".into());
                    }
                    let latest = peers::get(&state.db, &peer.id)
                        .map_err(|e| e.to_string())?
                        .ok_or("Peer not found")?;
                    if serde_json::to_value(&latest).map_err(|e| e.to_string())?
                        != serde_json::to_value(&peer).map_err(|e| e.to_string())?
                    {
                        return Err("Peer changed before send".into());
                    }
                    Ok(())
                },
            )
            .await;
            Ok::<Value, anyhow::Error>(match response {
                Ok(value)
                    if value["data"]["channelSystemMessage"].as_bool() == Some(true)
                        && value.get("errors").is_none_or(|v| {
                            v.is_null() || v.as_array().is_some_and(Vec::is_empty)
                        }) =>
                {
                    value
                }
                Ok(value)
                    if value
                        .get("errors")
                        .is_some_and(|v| v.as_array().is_some_and(|v| !v.is_empty())) =>
                {
                    value
                }
                Ok(_) => rejected("Channel message rejected"),
                Err(error) => rejected(error),
            })
        }))
        .await;
        for result in results {
            let result = result.unwrap_or_else(rejected);
            if result.get("errors").is_some() {
                response = result;
            }
        }
    }
    Ok(response)
}
