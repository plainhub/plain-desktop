use super::{
    channel::{chat_helper::ChannelDeliveryResult, messages::ChannelMember},
    enums::ChannelStatus,
    transport::{PeerTransport, chat_item_request},
};
use crate::db::{
    DChat, Db,
    chat_store::{channels, messages, peers},
};
use anyhow::{Result, bail};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

#[derive(serde::Serialize)]
pub struct Receipt {
    pub chat: Option<DChat>,
    pub rediscover: bool,
}

pub struct Delivery {
    db: Db,
    locks: Mutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>,
}
impl Delivery {
    pub fn new(db: Db) -> Self {
        Self {
            db,
            locks: Mutex::new(HashMap::new()),
        }
    }
    fn message_lock(&self, id: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.locks.lock().unwrap();
        locks.retain(|_, lock| lock.strong_count() != 0);
        if let Some(lock) = locks.get(id).and_then(Weak::upgrade) {
            return lock;
        }
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        locks.insert(id.into(), Arc::downgrade(&lock));
        lock
    }
    pub async fn send<T: PeerTransport>(
        &self,
        transport: &T,
        client_id: &str,
        signing_key: &[u8],
        url_token: &str,
        id: &str,
        recipients: Option<Vec<String>>,
    ) -> Result<Receipt> {
        self.send_observed(
            transport,
            client_id,
            signing_key,
            url_token,
            id,
            recipients,
            |_| {},
        )
        .await
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn send_observed<T: PeerTransport>(
        &self,
        transport: &T,
        client_id: &str,
        signing_key: &[u8],
        url_token: &str,
        id: &str,
        recipients: Option<Vec<String>>,
        pending: impl Fn(&DChat) + Send,
    ) -> Result<Receipt> {
        let lock = self.message_lock(id);
        let _guard = lock.lock().await;
        let Some(mut chat) = messages::get(&self.db, id)? else {
            return Ok(Receipt {
                chat: None,
                rediscover: false,
            });
        };
        if chat.from_id != "me" {
            bail!("Only outgoing messages can be sent");
        }
        let retry = recipients.is_some();
        if chat.channel_id.is_empty() && (chat.to_id.is_empty() || chat.to_id == "local") {
            if retry {
                bail!("Targeted retry requires a channel");
            }
            return Ok(Receipt {
                chat: Some(chat),
                rediscover: false,
            });
        }
        let content = super::content::peer_content(&chat.content, url_token)?;
        let targets = if chat.channel_id.is_empty() {
            if retry {
                bail!("Targeted retry requires a channel");
            }
            vec![chat.to_id.clone()]
        } else {
            let Some(channel) = channels::get(&self.db, &chat.channel_id)? else {
                return self.finish(&chat, None, retry, true);
            };
            let members: Vec<ChannelMember> = serde_json::from_str(&channel.members)?;
            if channel.status != ChannelStatus::Joined
                || !members
                    .iter()
                    .any(|m| m.peer_id == client_id && m.is_joined())
            {
                return self.finish(&chat, None, retry, true);
            }
            let joined: HashSet<_> = members
                .iter()
                .filter(|m| m.is_joined() && m.peer_id != client_id)
                .map(|m| m.peer_id.clone())
                .collect();
            match recipients {
                Some(ids) => {
                    let mut unique = HashSet::new();
                    if ids
                        .iter()
                        .any(|id| !joined.contains(id) || !unique.insert(id))
                    {
                        bail!("Invalid retry recipients");
                    }
                    ids
                }
                None => members
                    .into_iter()
                    .filter(|m| m.is_joined() && m.peer_id != client_id)
                    .map(|m| m.peer_id)
                    .collect(),
            }
        };
        chat.status = super::enums::ChatStatus::Pending;
        chat.updated_at = crate::db::now_iso();
        self.db.with_conn(|connection|->Result<()> {
            let changed=connection.execute("UPDATE chats SET status='PENDING',updated_at=?3 WHERE id=?1 AND content=?2 AND from_id='me'",rusqlite::params![chat.id,chat.content,chat.updated_at])?;
            if changed!=1 {bail!("Message changed before delivery");}Ok(())
        })?;
        pending(&chat);
        let mut results = Vec::with_capacity(targets.len());
        let mut rediscover = false;
        for target in targets {
            let peer = peers::get(&self.db, &target)?;
            let name = peer
                .as_ref()
                .map(|p| p.name.clone())
                .unwrap_or_else(|| target.clone());
            let error = match peer {
                None => Some("Peer not found in database".into()),
                Some(peer) => {
                    let key = if chat.channel_id.is_empty() {
                        if !peer.is_paired() {
                            results.push(ChannelDeliveryResult {
                                peer_id: target,
                                peer_name: name,
                                error: Some("peer unpaired".into()),
                            });
                            continue;
                        }
                        peer.key.clone()
                    } else {
                        let channel =
                            channels::get(&self.db, &chat.channel_id)?.ok_or_else(|| {
                                anyhow::anyhow!("Channel unavailable during delivery")
                            })?;
                        let members: Vec<ChannelMember> = serde_json::from_str(&channel.members)?;
                        if channel.status != ChannelStatus::Joined
                            || !members
                                .iter()
                                .any(|m| m.peer_id == client_id && m.is_joined())
                            || !members.iter().any(|m| m.peer_id == target && m.is_joined())
                        {
                            bail!("Channel membership changed during delivery");
                        }
                        channel.key
                    };
                    let key = crate::base64_decode(&key);
                    if key.len() != 32 {
                        Some("Invalid peer transport key".into())
                    } else {
                        let body =
                            chat_item_request(signing_key, &content).map_err(anyhow::Error::msg)?;
                        match tokio::time::timeout(
                            Duration::from_secs(20),
                            transport.message(&peer, client_id, &chat.channel_id, &key, &body),
                        )
                        .await
                        {
                            Ok(result) => result.err(),
                            Err(_) => Some("Peer delivery timed out after 20s".into()),
                        }
                    }
                }
            };
            rediscover |= error.is_some();
            results.push(ChannelDeliveryResult {
                peer_id: target,
                peer_name: name,
                error,
            });
        }
        self.finish(&chat, Some(results), retry, rediscover)
    }
    fn finish(
        &self,
        chat: &DChat,
        results: Option<Vec<ChannelDeliveryResult>>,
        retry: bool,
        rediscover: bool,
    ) -> Result<Receipt> {
        Ok(Receipt {
            chat: super::message_lifecycle::delivery_for_content(
                &self.db,
                &chat.id,
                results,
                retry,
                &chat.content,
            )?,
            rediscover,
        })
    }
}

#[cfg(test)]
#[path = "../../tests/unit/chat/delivery.rs"]
mod tests;
