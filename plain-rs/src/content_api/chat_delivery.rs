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
struct Transport(ServerState);
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
            .send(
                &Transport(state.clone()),
                &client_id,
                &key,
                &token,
                &request.id,
                request.recipients,
            )
            .await
    }
    .await;
    match result {
        Ok(receipt) => Json(json!({"result":receipt})).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":error.to_string()})),
        )
            .into_response(),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/chat_delivery.rs"]
mod tests;
