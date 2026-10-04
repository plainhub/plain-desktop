use crate::{
    chat::{
        channel::{messages::ChannelMember, outgoing},
        enums::{ChannelSystemMessageType, DeviceType},
        transport::channel_system_request,
    },
    db::{DChannel, DPeer, Db, chat_store::peers},
    prefs::Prefs,
};
use anyhow::{Result, bail};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Prepared {
    pub payload: String,
    pub message_type: ChannelSystemMessageType,
    pub targets: Vec<Target>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Target {
    pub peer: DPeer,
    pub key: String,
    pub channel_id: String,
}
pub(super) fn prepare(
    db: &Db,
    prefs: &Prefs,
    channel: &DChannel,
    kind: ChannelSystemMessageType,
    target: &str,
    name: &str,
    device: DeviceType,
) -> Result<Prepared> {
    if matches!(
        kind,
        ChannelSystemMessageType::InviteAccept
            | ChannelSystemMessageType::InviteDecline
            | ChannelSystemMessageType::Leave
    ) && (target.is_empty() || target != channel.owner_id)
    {
        bail!("Invalid channel owner destination");
    }
    let actor = prefs
        .get::<String>("client_id")?
        .filter(|id| !id.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Missing client identity"))?;
    let keypair = signing_keypair(prefs)?;
    let payload = outgoing::build(
        Some(db),
        channel,
        &actor,
        name,
        device,
        &keypair,
        kind,
        target,
    )?;
    let ids = if !target.is_empty() {
        vec![target.to_owned()]
    } else {
        serde_json::from_str::<Vec<ChannelMember>>(&channel.members)?
            .into_iter()
            .filter(|m| m.peer_id != actor)
            .map(|m| m.peer_id)
            .collect()
    };
    let mut targets = Vec::new();
    for id in ids {
        let Some(peer) = peers::get(db, &id)? else {
            continue;
        };
        let (key, cid) = if !peer.key.is_empty() {
            (peer.key.clone(), String::new())
        } else {
            (channel.key.clone(), channel.id.clone())
        };
        if crate::base64_decode(&key).len() != 32 {
            bail!("Invalid channel transport key for {}", peer.id);
        }
        targets.push(Target {
            peer,
            key,
            channel_id: cid,
        });
    }
    Ok(Prepared {
        payload,
        message_type: kind,
        targets,
    })
}
fn signing_keypair(prefs: &Prefs) -> Result<Vec<u8>> {
    let raw = prefs
        .get::<String>("signature_key_pair")?
        .ok_or_else(|| anyhow::anyhow!("Missing signature keypair"))?;
    let pair: serde_json::Value = serde_json::from_str(&raw)?;
    let mut keypair = crate::base64_decode(
        pair["privateKey"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing private key"))?,
    );
    let public = crate::base64_decode(
        pair["publicKey"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing public key"))?,
    );
    if keypair.len() != 32 || public.len() != 32 {
        bail!("Invalid signature keypair");
    }
    keypair.extend(public);
    Ok(keypair)
}
pub(super) fn wire(prefs: &Prefs, kind: ChannelSystemMessageType, payload: &str) -> Result<String> {
    channel_system_request(&signing_keypair(prefs)?, kind.as_str(), payload)
        .map_err(anyhow::Error::msg)
}
#[cfg(test)]
#[path = "../../tests/unit/content_api/channel_outgoing.rs"]
mod tests;
