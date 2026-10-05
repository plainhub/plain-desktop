use super::server::ServerState;
use crate::{
    chat::transport_router::{Outcome, Router, Ticket, TransportType},
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
fn notify(state: &ServerState) {
    let _ = state
        .events
        .send(crate::ws_event::WsEvent::broadcast(10003, "".into()));
}
struct Pending<'a> {
    router: &'a Router,
    state: &'a ServerState,
    ticket: Ticket,
}
impl Drop for Pending<'_> {
    fn drop(&mut self) {
        self.router.abort(&self.ticket);
        notify(self.state);
    }
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Attempt {
    Connected { response: Value },
    Unavailable { error: String },
}
pub(super) async fn send(
    state: &ServerState,
    peer: &DPeer,
    channel_id: &str,
    key: &[u8],
    body: &str,
) -> Result<Value, String> {
    send_checked(state, peer, channel_id, key, body, || Ok(())).await
}
pub(super) async fn send_checked(
    state: &ServerState,
    peer: &DPeer,
    channel_id: &str,
    key: &[u8],
    body: &str,
    validate: impl Fn() -> Result<(), String>,
) -> Result<Value, String> {
    if key.len() != 32 {
        return Err("Invalid peer transport key".into());
    }
    if *state.stop.borrow() {
        return Err("Core stopped".into());
    }
    let host = &state.host;
    let router = &state.transport;
    let mut available = vec![TransportType::Lan];
    if host.connected() {
        let sdk = host.call("peerTransportCapabilities", json!({})).await?;
        let sdk: Vec<TransportType> = serde_json::from_value(sdk).map_err(|e| e.to_string())?;
        available.extend(sdk.into_iter().filter(|kind| *kind != TransportType::Lan));
    }
    let mut step = router.begin(peer, &available).map_err(|e| e.to_string())?;
    let first = step.ticket.take().ok_or_else(|| {
        step.error
            .unwrap_or_else(|| "Peer transport unavailable".into())
    })?;
    let mut pending = Pending {
        state,
        router,
        ticket: first,
    };
    loop {
        notify(state);
        validate()?;
        super::peer_address::current(&state.db, &peer.id, peer).map_err(|e| e.to_string())?;
        if pending.ticket.transport == TransportType::Lan {
            let mut stop = state.stop.clone();
            let response = tokio::select! {_=stop.changed()=>return Err("Core stopped".into()),result=super::peer_lan::send(state,peer,channel_id,key,body,&validate)=>result};
            match response {
                Ok(response) => {
                    validate()?;
                    super::peer_address::current(&state.db, &peer.id, peer)
                        .map_err(|e| e.to_string())?;
                    router
                        .finish(&pending.ticket, Outcome::Connected)
                        .map_err(|e| e.to_string())?;
                    return Ok(response);
                }
                Err(super::peer_lan::Failure::Fatal(error)) => return Err(error),
                Err(super::peer_lan::Failure::Unavailable(error)) => {
                    let step = router
                        .finish(&pending.ticket, Outcome::Unavailable { error })
                        .map_err(|e| e.to_string())?;
                    match step.ticket {
                        Some(ticket) => {
                            pending.ticket = ticket;
                            continue;
                        }
                        None => {
                            return Err(step
                                .error
                                .unwrap_or_else(|| "Peer transport unavailable".into()));
                        }
                    }
                }
            }
        }
        let timeout_ms = 15_000;
        let attempt=tokio::time::timeout(Duration::from_millis(timeout_ms),host.call("peerTransportAttempt",json!({"transport":pending.ticket.transport,"timeoutMs":timeout_ms,"peer":super::peer_address::view(peer),"channelId":channel_id,"key":crate::base64_encode(key),"body":body}))).await;
        let outcome = match attempt {
            Err(_) => Outcome::Unavailable {
                error: "Transport attempt timed out after 15s".into(),
            },
            Ok(Err(error)) => return Err(error),
            Ok(Ok(value)) => {
                match serde_json::from_value::<Attempt>(value).map_err(|e| e.to_string())? {
                    Attempt::Connected { response } => {
                        validate()?;
                        super::peer_address::current(&state.db, &peer.id, peer)
                            .map_err(|e| e.to_string())?;
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
    SnapshotTransfers,
}
pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let notify_result = !matches!(request, Request::SnapshotTransfers);
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
                    &state,
                    &peer,
                    &channel_id,
                    &crate::base64_decode(&key),
                    &body,
                )
                .await
                .map_err(anyhow::Error::msg)?
            }
            Request::BeginDownload { id, mut available } => {
                if !available.contains(&TransportType::Lan) { available.push(TransportType::Lan); }
                let peer =
                    peers::get(&state.db, &id)?.ok_or_else(|| anyhow::anyhow!("Unknown peer"))?;
                let step = state.transport.begin(&peer, &available)?;
                json!({"peer":super::peer_address::view(&peer),"ticket":step.ticket,"error":step.error})
            }
            Request::FinishDownload { ticket, outcome } => {
                serde_json::to_value(state.transport.finish(&ticket, outcome)?)?
            }
            Request::SnapshotTransfers => serde_json::to_value(state.transport.active())?,
            Request::Abort { ticket } => {
                state.transport.abort(&ticket);
                json!(true)
            }
        })
    }
    .await;
    if notify_result && result.is_ok() {
        notify(&state);
    }
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
