use super::{incoming_store as store, messages::*};
use crate::{
    chat::enums::{
        ChannelStatus, ChannelSystemMessageAction as Action, ChannelSystemMessageType as Type,
        MemberStatus,
    },
    db::{DChannel, Db, now_iso},
};
use anyhow::{Result, bail};

#[derive(Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Received {
    pub accepted: bool,
    pub changed: bool,
    pub broadcast: bool,
    pub channel: Option<DChannel>,
    pub invite: Option<Invite>,
    pub cancel: Option<Cancel>,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Invite {
    pub channel_id: String,
    pub channel_name: String,
    pub owner_peer_id: String,
    pub owner_peer_name: String,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cancel {
    pub channel_id: String,
    pub owner_peer_id: String,
}

pub fn verify(public_key: &str, payload: &str, signature: &str) -> bool {
    !public_key.is_empty()
        && !signature.is_empty()
        && crate::ed25519_verify(public_key, payload.as_bytes(), signature)
}
fn signature(
    key: &str,
    id: &str,
    version: i64,
    action: Action,
    target: &str,
    sig: &str,
) -> Result<()> {
    if version < 0
        || !verify(
            key,
            &channel_message_payload(id, version, action, target),
            sig,
        )
    {
        bail!("Invalid channel signature");
    }
    Ok(())
}
fn validate_members(members: &[ChannelMember]) -> Result<()> {
    let mut ids = std::collections::HashSet::new();
    if members
        .iter()
        .any(|m| m.peer_id.is_empty() || !ids.insert(&m.peer_id))
    {
        bail!("Invalid channel member roster");
    }
    Ok(())
}
fn advance(channel: &mut DChannel) -> Result<()> {
    channel.version = channel
        .version
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("Channel version overflow"))?;
    channel.updated_at = now_iso();
    Ok(())
}

pub fn receive(db: &Db, actor: &str, from: &str, kind: Type, payload: &str) -> Result<Received> {
    if actor.is_empty() || from.is_empty() {
        bail!("Missing channel message identity");
    }
    let value: serde_json::Value = serde_json::from_str(payload)?;
    let id = value["channelId"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Missing channel id"))?
        .to_owned();
    let id = id.as_str();
    db.with_conn(|c| -> Result<Received> {
        let tx = c.unchecked_transaction()?;
        let current = store::channel(&tx, id)?;
        let mut result = Received {
            accepted: true,
            ..Default::default()
        };
        match kind {
            Type::Invite => {
                let msg: ChannelInvite = serde_json::from_value(value)?;
                if msg.owner != from {
                    bail!("Invite sender is not owner");
                }
                let owner = store::peer(&tx, from)?
                    .ok_or_else(|| anyhow::anyhow!("Unknown invite owner"))?;
                let info = msg
                    .member_peers
                    .iter()
                    .find(|p| p.id == from)
                    .ok_or_else(|| anyhow::anyhow!("Missing invite owner information"))?;
                if crate::base64_decode(&info.public_key) != crate::base64_decode(&owner.public_key)
                {
                    bail!("Invite owner public key mismatch");
                }
                signature(
                    &owner.public_key,
                    id,
                    msg.version,
                    Action::Invite,
                    actor,
                    &msg.signature,
                )?;
                if crate::base64_decode(&msg.key).len() != 32 {
                    bail!("Invalid channel key");
                }
                validate_members(&msg.members)?;
                if !msg
                    .members
                    .iter()
                    .any(|m| m.peer_id == from && m.is_joined())
                    || !msg
                        .members
                        .iter()
                        .any(|m| m.peer_id == actor && m.is_pending())
                {
                    bail!("Invalid invitation membership");
                }
                let new = current.is_none();
                let mut channel = match current {
                    Some(channel) => {
                        if channel.owner_id != from {
                            bail!("Channel owner mismatch");
                        }
                        if !matches!(channel.status, ChannelStatus::Left | ChannelStatus::Kicked) {
                            return Ok(result);
                        }
                        if msg.version <= channel.version {
                            bail!("Stale channel invitation");
                        }
                        channel
                    }
                    None => {
                        let mut channel = DChannel::new(&msg.channel_name, from);
                        channel.id = msg.channel_id.clone();
                        channel
                    }
                };
                store::ensure_peers(&tx, &msg.member_peers, actor)?;
                channel.name = msg.channel_name;
                channel.key = msg.key;
                channel.members = serde_json::to_string(&msg.members)?;
                channel.version = msg.version;
                channel.status = ChannelStatus::Joined;
                channel.updated_at = now_iso();
                store::save(&tx, &channel, new)?;
                result.invite = Some(Invite {
                    channel_id: id.into(),
                    channel_name: channel.name.clone(),
                    owner_peer_id: from.into(),
                    owner_peer_name: if owner.name.is_empty() {
                        from.into()
                    } else {
                        owner.name
                    },
                });
                result.channel = Some(channel);
            }
            Type::Update => {
                let msg: ChannelUpdate = serde_json::from_value(value)?;
                let mut channel = current.ok_or_else(|| anyhow::anyhow!("Unknown channel"))?;
                if channel.owner_id != from {
                    bail!("Update sender is not owner");
                }
                let owner = store::peer(&tx, from)?
                    .ok_or_else(|| anyhow::anyhow!("Unknown channel owner"))?;
                signature(
                    &owner.public_key,
                    id,
                    msg.version,
                    Action::Update,
                    "",
                    &msg.signature,
                )?;
                if msg.version <= channel.version {
                    return Ok(result);
                }
                validate_members(&msg.members)?;
                store::ensure_peers(&tx, &msg.member_peers, actor)?;
                channel.name = msg.channel_name;
                channel.members = serde_json::to_string(&msg.members)?;
                channel.version = msg.version;
                channel.updated_at = now_iso();
                store::save(&tx, &channel, false)?;
                result.channel = Some(channel);
            }
            Type::Kick => {
                let msg: ChannelKick = serde_json::from_value(value)?;
                let owner = store::peer(&tx, from)?
                    .ok_or_else(|| anyhow::anyhow!("Unknown channel owner"))?;
                if signature(
                    &owner.public_key,
                    id,
                    msg.version,
                    Action::Kick,
                    actor,
                    &msg.signature,
                )
                .is_err()
                {
                    signature(
                        &owner.public_key,
                        id,
                        msg.version,
                        Action::Kick,
                        "",
                        &msg.signature,
                    )?;
                }
                if let Some(mut channel) = current {
                    if channel.owner_id != from {
                        bail!("Kick sender is not owner");
                    }
                    if msg.version < channel.version {
                        return Ok(result);
                    }
                    let mut members = store::members(&channel.members)?;
                    let pending = members.iter().any(|m| m.peer_id == actor && m.is_pending());
                    if channel.status == ChannelStatus::Kicked
                        && !members.iter().any(|m| m.peer_id == actor)
                    {
                        return Ok(result);
                    }
                    members.retain(|m| m.peer_id != actor);
                    channel.members = serde_json::to_string(&members)?;
                    channel.status = ChannelStatus::Kicked;
                    channel.version = msg.version;
                    channel.updated_at = now_iso();
                    store::save(&tx, &channel, false)?;
                    result.channel = Some(channel);
                    if pending {
                        result.cancel = Some(Cancel {
                            channel_id: id.into(),
                            owner_peer_id: from.into(),
                        });
                    }
                } else {
                    result.cancel = Some(Cancel {
                        channel_id: id.into(),
                        owner_peer_id: from.into(),
                    });
                }
            }
            Type::InviteAccept | Type::InviteDecline | Type::Leave => {
                let mut channel = current.ok_or_else(|| anyhow::anyhow!("Unknown channel"))?;
                if channel.owner_id != actor || from == actor {
                    bail!("Invalid member message ownership");
                }
                let mut members = store::members(&channel.members)?;
                let Some(index) = members.iter().position(|m| m.peer_id == from) else {
                    return Ok(result);
                };
                match kind {
                    Type::InviteAccept => {
                        let msg: ChannelInviteAccept = serde_json::from_value(value)?;
                        if !members[index].is_pending() {
                            return Ok(result);
                        }
                        store::accepting_peer(
                            &tx,
                            from,
                            &msg.name,
                            &msg.public_key,
                            msg.device_type,
                        )?;
                        members[index].status = MemberStatus::Joined;
                        result.broadcast = true;
                    }
                    Type::InviteDecline => {
                        let _: ChannelInviteDecline = serde_json::from_value(value)?;
                        if !members[index].is_pending() {
                            return Ok(result);
                        }
                        members.remove(index);
                    }
                    Type::Leave => {
                        let _: ChannelLeave = serde_json::from_value(value)?;
                        members.remove(index);
                        result.broadcast = true;
                    }
                    _ => unreachable!(),
                }
                channel.members = serde_json::to_string(&members)?;
                advance(&mut channel)?;
                store::save(&tx, &channel, false)?;
                result.channel = Some(channel);
            }
        }
        result.changed = result.channel.is_some();
        tx.commit()?;
        Ok(result)
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/chat/channel/incoming.rs"]
mod tests;
