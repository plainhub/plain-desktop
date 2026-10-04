use super::{host::Host, server::ServerState};
use crate::{
    chat::transport_router::{Outcome, Router, Step, Ticket, TransportType},
    db::{DPeer, chat_store::peers},
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Duration;
struct Pending<'a> {
    router: &'a Router,
    ticket: Ticket,
}
impl Drop for Pending<'_> {
    fn drop(&mut self) {
        self.router.abort(&self.ticket);
    }
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Attempt {
    Connected { response: Value },
    Unavailable { error: String },
}
pub(super) async fn send(
    host: &Host,
    router: &Router,
    peer: &DPeer,
    channel_id: &str,
    key: &[u8],
    body: &str,
) -> Result<Value, String> {
    if key.len() != 32 {
        return Err("Invalid peer transport key".into());
    }
    let available = host.call("peerTransportCapabilities", json!({})).await?;
    let available: Vec<TransportType> =
        serde_json::from_value(available).map_err(|e| e.to_string())?;
    let mut step = router.begin(peer, &available).map_err(|e| e.to_string())?;
    let first = step.ticket.take().ok_or_else(|| {
        step.error
            .unwrap_or_else(|| "Peer transport unavailable".into())
    })?;
    let mut pending = Pending {
        router,
        ticket: first,
    };
    loop {
        let timeout_ms = 15_000;
        let attempt=tokio::time::timeout(Duration::from_millis(timeout_ms),host.call("peerTransportAttempt",json!({"transport":pending.ticket.transport,"timeoutMs":timeout_ms,"peer":peer,"channelId":channel_id,"key":crate::base64_encode(key),"body":body}))).await;
        let outcome = match attempt {
            Err(_) => Outcome::Unavailable {
                error: "Transport attempt timed out after 15s".into(),
            },
            Ok(Err(error)) => return Err(error),
            Ok(Ok(value)) => {
                match serde_json::from_value::<Attempt>(value).map_err(|e| e.to_string())? {
                    Attempt::Connected { response } => {
                        router
                            .finish(&pending.ticket, Outcome::Connected)
                            .map_err(|e| e.to_string())?;
                        return Ok(response);
                    }
                    Attempt::Unavailable { error } => Outcome::Unavailable { error },
                }
            }
        };
        let step = router
            .finish(&pending.ticket, outcome)
            .map_err(|e| e.to_string())?;
        match step.ticket {
            Some(ticket) => pending.ticket = ticket,
            None => {
                return Err(step
                    .error
                    .unwrap_or_else(|| "Peer transport unavailable".into()));
            }
        }
    }
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Request {
    Send {
        id: String,
        channel_id: String,
        key: String,
        body: String,
    },
    BeginDownload {
        id: String,
        available: Vec<TransportType>,
    },
    FinishDownload {
        ticket: Ticket,
        outcome: Outcome,
    },
    Abort {
        ticket: Ticket,
    },
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
        Ok::<Value, anyhow::Error>(match request {
            Request::Send {
                id,
                channel_id,
                key,
                body,
            } => {
                let peer =
                    peers::get(&state.db, &id)?.ok_or_else(|| anyhow::anyhow!("Unknown peer"))?;
                send(
                    &state.host,
                    &state.transport,
                    &peer,
                    &channel_id,
                    &crate::base64_decode(&key),
                    &body,
                )
                .await
                .map_err(anyhow::Error::msg)?
            }
            Request::BeginDownload { id, available } => {
                let peer =
                    peers::get(&state.db, &id)?.ok_or_else(|| anyhow::anyhow!("Unknown peer"))?;
                let step = state.transport.begin(&peer, &available)?;
                json!({"peer":peer,"ticket":step.ticket,"error":step.error})
            }
            Request::FinishDownload { ticket, outcome } => {
                serde_json::to_value(state.transport.finish(&ticket, outcome)?)?
            }
            Request::Abort { ticket } => {
                state.transport.abort(&ticket);
                json!(true)
            }
        })
    }
    .await;
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
#[path = "../../tests/unit/content_api/peer_transport.rs"]
mod tests;
