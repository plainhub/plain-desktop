use super::{peer_transport, peer_wire, server::ServerState};
use crate::chat::prewarm::{self, Advertisement, Capabilities, Driver};
use anyhow::{Result, bail};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};

struct Native<'a>(&'a ServerState);
impl Driver for Native<'_> {
    async fn capabilities(&self) -> Result<Capabilities> {
        Ok(serde_json::from_value(
            self.0
                .host
                .call("peerTransportPrewarmCapabilities", json!({}))
                .await
                .map_err(anyhow::Error::msg)?,
        )?)
    }
    async fn scan(&self, short_id: &str) -> Result<Option<Advertisement>> {
        Ok(serde_json::from_value(
            self.0
                .host
                .call(
                    "peerTransportPrewarmScan",
                    json!({"shortId":short_id,"timeoutMs":15000}),
                )
                .await
                .map_err(anyhow::Error::msg)?,
        )?)
    }
    async fn start_aware(&self, peer_id: &str) -> Result<bool> {
        let prepared = peer_wire::prepare(
            &self.0.db,
            &self.0.prefs,
            peer_id,
            peer_wire::Operation::StartAware,
        )?;
        if !prepared.peer.is_paired() {
            bail!("Peer no longer paired")
        }
        let response = peer_transport::send(
            &self.0.host,
            &self.0.transport,
            &prepared.peer,
            &prepared.channel_id,
            &crate::base64_decode(&prepared.key),
            &prepared.body,
        )
        .await
        .map_err(anyhow::Error::msg)?;
        started(&response)
    }
    async fn observe(&self, peer_id: &str, value: &Advertisement) -> Result<()> {
        self.0
            .host
            .call(
                "peerTransportPrewarmObservation",
                json!({"id":peer_id,"advertisement":value}),
            )
            .await
            .map_err(anyhow::Error::msg)?;
        Ok(())
    }
}
fn started(response: &Value) -> Result<bool> {
    if response
        .get("errors")
        .is_some_and(|v| !v.is_null() && !v.as_array().is_some_and(Vec::is_empty))
    {
        bail!("Peer startAware returned GraphQL errors");
    }
    response["data"]["startAware"]
        .as_bool()
        .ok_or_else(|| anyhow::anyhow!("Invalid peer startAware receipt"))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    id: String,
}
pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let result = prewarm::run(&state.db, &state.prewarmer, &request.id, &Native(&state)).await;
    match result {
        Ok(value) => Json(json!({"result":value})).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":error.to_string()})),
        )
            .into_response(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn start_receipt_requires_actual_boolean_and_no_errors() {
        assert!(!started(&json!({"data":{"startAware":false}})).unwrap());
        assert!(started(&json!({"data":{"startAware":true},"errors":[]})).unwrap());
        assert!(
            started(&json!({"data":{"startAware":true},"errors":[{"message":"failed"}]})).is_err()
        );
        assert!(started(&json!({"data":{}})).is_err());
    }
}
