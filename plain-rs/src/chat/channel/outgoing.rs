use super::{incoming_store as store, messages::*};
use crate::{
    base64_encode,
    chat::enums::{
        ChannelSystemMessageAction as Action, ChannelSystemMessageType as Type, DeviceType,
    },
    db::{DChannel, Db},
    ed25519_sign,
};
use anyhow::{Result, bail};

pub fn build(
    db: Option<&Db>,
    channel: &DChannel,
    actor: &str,
    name: &str,
    device: DeviceType,
    keypair: &[u8],
    kind: Type,
    target: &str,
) -> Result<String> {
    if actor.is_empty() || channel.id.is_empty() || keypair.len() != 64 {
        bail!("Invalid channel sender identity");
    }
    let signature = |action, target| -> Result<String> {
        let payload = channel_message_payload(&channel.id, channel.version, action, target);
        let sig = ed25519_sign(keypair, payload.as_bytes());
        if !crate::ed25519_verify(&base64_encode(&keypair[32..]), payload.as_bytes(), &sig) {
            bail!("Invalid channel signing keypair");
        }
        Ok(sig)
    };
    Ok(match kind {
        Type::Invite | Type::Update => {
            if channel.owner_id != actor {
                bail!("Only owner can send channel membership");
            }
            let members: Vec<ChannelMember> = serde_json::from_str(&channel.members)?;
            let member_peers = db
                .ok_or_else(|| anyhow::anyhow!("Missing channel database"))?
                .with_conn(|c| -> Result<Vec<MemberPeerInfo>> {
                    let mut peers = vec![MemberPeerInfo {
                        id: actor.into(),
                        name: name.into(),
                        public_key: base64_encode(&keypair[32..]),
                        device_type: device,
                        ip: String::new(),
                        port: 0,
                    }];
                    for member in &members {
                        if member.peer_id == actor {
                            continue;
                        }
                        if let Some(peer) = store::peer(c, &member.peer_id)? {
                            peers.push(MemberPeerInfo {
                                id: peer.id,
                                name: peer.name,
                                public_key: peer.public_key,
                                device_type: peer.device_type,
                                ip: peer.ip,
                                port: peer.port,
                            });
                        }
                    }
                    Ok(peers)
                })?;
            if kind == Type::Invite {
                if target.is_empty() {
                    bail!("Missing invitation target");
                }
                serde_json::to_string(&ChannelInvite {
                    channel_id: channel.id.clone(),
                    channel_name: channel.name.clone(),
                    key: channel.key.clone(),
                    owner: actor.into(),
                    members,
                    member_peers,
                    version: channel.version,
                    signature: signature(Action::Invite, target)?,
                })?
            } else {
                serde_json::to_string(&ChannelUpdate {
                    channel_id: channel.id.clone(),
                    channel_name: channel.name.clone(),
                    members,
                    member_peers,
                    version: channel.version,
                    signature: signature(Action::Update, "")?,
                })?
            }
        }
        Type::Kick => {
            if channel.owner_id != actor {
                bail!("Only owner can remove channel members");
            }
            serde_json::to_string(&ChannelKick {
                channel_id: channel.id.clone(),
                version: channel.version,
                signature: signature(Action::Kick, target)?,
            })?
        }
        Type::InviteAccept => serde_json::to_string(&ChannelInviteAccept {
            channel_id: channel.id.clone(),
            public_key: base64_encode(&keypair[32..]),
            name: name.into(),
            device_type: device,
        })?,
        Type::InviteDecline => serde_json::to_string(&ChannelInviteDecline {
            channel_id: channel.id.clone(),
        })?,
        Type::Leave => serde_json::to_string(&ChannelLeave {
            channel_id: channel.id.clone(),
        })?,
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/chat/channel/outgoing.rs"]
mod tests;
