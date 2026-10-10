use crate::adapters::mobile::runtime::server::ServerState;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use plain_rs::{
    chat::transport::{PeerTransport, message_response},
    db::DPeer,
};
use serde::Deserialize;
use serde_json::json;
pub(in crate::adapters::mobile) struct Transport(pub(in crate::adapters::mobile) ServerState);
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
        let response = crate::adapters::mobile::peers::peer_transport::send(
            &self.0, peer, channel_id, key, body,
        )
        .await?;
        message_response(&response)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::adapters::mobile) struct Request {
    id: String,
    recipients: Option<Vec<String>>,
}
pub(in crate::adapters::mobile) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let result = deliver(&state, &request.id, request.recipients).await;
    match result {
        Ok(receipt) => Json(json!({"result":receipt})).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":error.to_string()})),
        )
            .into_response(),
    }
}

pub(in crate::adapters::mobile) async fn deliver(
    state: &ServerState,
    id: &str,
    recipients: Option<Vec<String>>,
) -> anyhow::Result<plain_rs::chat::delivery::Receipt> {
    deliver_observed(state, id, recipients, |_| {}).await
}
pub(in crate::adapters::mobile) async fn deliver_observed(
    state: &ServerState,
    id: &str,
    recipients: Option<Vec<String>>,
    pending: impl Fn(&plain_rs::db::DChat) + Send + Sync,
) -> anyhow::Result<plain_rs::chat::delivery::Receipt> {
    let result = async {
        let client_id = state
            .prefs
            .get::<String>("client_id")?
            .filter(|v| !v.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Missing chat identity"))?;
        let key = crate::adapters::mobile::peers::peer_wire::signing_keypair(&state.prefs)?;
        let token = state.prefs.get::<String>("url_token")?.unwrap_or_default();
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
                    let _ = state.events.send(plain_rs::ws_event::WsEvent::broadcast(
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
        let _ = state.events.send(plain_rs::ws_event::WsEvent::broadcast(
            plain_rs::chat::events::WS_MESSAGE_UPDATED,
            serde_json::json!([plain_rs::chat::service::chat_to_json(chat)]).to_string(),
        ));
    }
    Ok(receipt)
}

#[cfg(test)]
#[path = "../../../../tests/unit/adapters/mobile/chat/chat_delivery.rs"]
mod tests;
