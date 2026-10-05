use super::server::ServerState;
use crate::{
    chat::transport::{PeerTransport, message_response},
    db::DPeer,
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
pub(super) struct Transport(pub(super) ServerState);
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
        let response = super::peer_transport::send(&self.0, peer, channel_id, key, body).await?;
        message_response(&response)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    id: String,
    recipients: Option<Vec<String>>,
}
pub(super) async fn call(
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

pub(super) async fn deliver(
    state: &ServerState,
    id: &str,
    recipients: Option<Vec<String>>,
) -> anyhow::Result<crate::chat::delivery::Receipt> {
    deliver_observed(state, id, recipients, |_| {}).await
}
pub(super) async fn deliver_observed(
    state: &ServerState,
    id: &str,
    recipients: Option<Vec<String>>,
    pending: impl Fn(&crate::db::DChat) + Send + Sync,
) -> anyhow::Result<crate::chat::delivery::Receipt> {
    let result = async {
        let client_id = state
            .prefs
            .get::<String>("client_id")?
            .filter(|v| !v.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Missing chat identity"))?;
        let key = super::peer_wire::signing_keypair(&state.prefs)?;
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
                    let _ = state.events.send(crate::ws_event::WsEvent::broadcast(
                        crate::chat::events::WS_MESSAGE_UPDATED,
                        serde_json::json!([crate::chat::service::chat_to_json(chat)]).to_string(),
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
        let _ = state.events.send(crate::ws_event::WsEvent::broadcast(
            crate::chat::events::WS_MESSAGE_UPDATED,
            serde_json::json!([crate::chat::service::chat_to_json(chat)]).to_string(),
        ));
    }
    Ok(receipt)
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/chat_delivery.rs"]
mod tests;
