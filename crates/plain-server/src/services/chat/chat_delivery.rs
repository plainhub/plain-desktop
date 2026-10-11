use crate::assembly::server::ServerState;
use plain_rs::db::DPeer;
use plain_rs::transport::contract::PeerTransport;
use plain_rs::transport::contract::message_response;
use serde_json::json;
pub(crate) struct Transport(pub(crate) ServerState);
impl PeerTransport for Transport {
    async fn post<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
        _: Option<&'a str>,
        _: &'a [u8],
    ) -> Result<Vec<u8>, String> {
        Err("Raw mobile HTTP transport unavailable".into())
    }
    async fn message(
        &self,
        peer: &DPeer,
        _: &str,
        channel_id: &str,
        key: &[u8],
        body: &str,
    ) -> Result<(), String> {
        let response =
            crate::services::peers::peer_transport::send(&self.0, peer, channel_id, key, body)
                .await?;
        message_response(&response)
    }
}
pub(crate) async fn deliver(
    state: &ServerState,
    id: &str,
    recipients: Option<Vec<String>>,
) -> anyhow::Result<plain_rs::chat::delivery::Receipt> {
    deliver_observed(state, id, recipients, |_| {}).await
}
pub(crate) async fn deliver_observed(
    state: &ServerState,
    id: &str,
    recipients: Option<Vec<String>>,
    pending: impl Fn(&plain_rs::db::DChat) + Send + Sync,
) -> anyhow::Result<plain_rs::chat::delivery::Receipt> {
    let result = async {
        let client_id = state
            .prefs
            .get::<String>(plain_rs::prefs::system::keys::CLIENT_ID)?
            .filter(|v| !v.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Missing chat identity"))?;
        let key = crate::services::peers::peer_wire::signing_keypair(&state.prefs)?;
        let token = state
            .prefs
            .get::<String>(plain_rs::prefs::system::keys::URL_TOKEN)?
            .unwrap_or_default();
        state
            .delivery
            .send_observed(
                &Transport(state.clone()),
                &client_id,
                &key,
                &token,
                id,
                recipients,
                |chat| {
                    pending(chat);
                    let _ = state.events.send(plain_rs::events::WsEvent::broadcast(
                        plain_rs::chat::events::WS_MESSAGE_UPDATED,
                        serde_json::json!([plain_rs::chat::service::chat_to_json(chat)])
                            .to_string(),
                    ));
                },
            )
            .await
    }
    .await;
    let receipt = result?;
    if receipt.rediscover {
        state.mdns.browse_resident();
    }
    if let Some(chat) = &receipt.chat {
        let _ = state.events.send(plain_rs::events::WsEvent::broadcast(
            plain_rs::chat::events::WS_MESSAGE_UPDATED,
            serde_json::json!([plain_rs::chat::service::chat_to_json(chat)]).to_string(),
        ));
    }
    Ok(receipt)
}

#[cfg(test)]
#[path = "../../../tests/unit/services/chat/chat_delivery.rs"]
mod tests;
