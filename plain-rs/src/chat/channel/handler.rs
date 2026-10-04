#[cfg(test)]
use super::{incoming::verify as verify_channel_signature, messages::channel_message_payload};
#[cfg(test)]
use crate::chat::enums::ChannelSystemMessageAction;
use crate::{
    base64_decode,
    chat::{
        enums::ChannelSystemMessageType,
        events::{
            ChatEvent, WS_CHANNEL_INVITE_RECEIVED, WS_CHANNELS_UPDATED, channels_updated_payload,
            load_key_cache,
        },
        service::ChatService,
        transport::PeerTransport,
    },
};
use serde_json::json;

pub fn handle<T: PeerTransport + 'static>(
    service: &ChatService<T>,
    from: &str,
    kind: ChannelSystemMessageType,
    payload: &str,
    kp_bytes: &[u8],
) -> bool {
    let result = match super::incoming::receive(
        &service.db,
        &service.identity.client_id,
        from,
        kind,
        payload,
    ) {
        Ok(result) => result,
        Err(error) => {
            log::warn!("channel message rejected: {error}");
            return false;
        }
    };
    if result.changed {
        load_key_cache(
            &service.db,
            &service.peer_key_cache,
            &service.channel_key_cache,
        );
        let _ = service.event_tx.send(ChatEvent {
            event_type: WS_CHANNELS_UPDATED,
            payload: channels_updated_payload(&service.db),
        });
    }
    if let Some(invite) = result.invite {
        let _=service.event_tx.send(ChatEvent {event_type:WS_CHANNEL_INVITE_RECEIVED,payload:json!({"channelId":invite.channel_id,"channelName":invite.channel_name,"fromId":invite.owner_peer_id,"fromName":invite.owner_peer_name}).to_string()});
    }
    if result.broadcast {
        if let Some(channel) = result.channel {
            let transport = service.transport.clone();
            let db = service.db.clone();
            let cache = service.peer_key_cache.clone();
            let actor = service.identity.client_id.clone();
            let name = service.identity.device_name();
            let kind = service.wire_device_type;
            let keypair = kp_bytes.to_vec();
            let key = base64_decode(&channel.key);
            tokio::spawn(async move {
                super::sender::broadcast_update(
                    &transport, &channel, &actor, &name, kind, &keypair, &db, &cache, &key,
                )
                .await;
            });
        }
    }
    result.accepted
}

#[cfg(test)]
#[path = "../../../tests/unit/chat/channel/handler.rs"]
mod tests;
