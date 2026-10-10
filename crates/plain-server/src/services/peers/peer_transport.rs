use crate::assembly::server::ServerState;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use plain_rs::db::DPeer;
use plain_rs::db::chat_store::peers;
use plain_rs::transport::router::Outcome;
use plain_rs::transport::router::Router;
use plain_rs::transport::router::Ticket;
use plain_rs::transport::router::TransportType;
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Duration;
pub(crate) fn notify(state: &ServerState) {
    let _ = state.events.send(plain_rs::events::WsEvent::broadcast(
        "PEER_TRANSPORT_UPDATED",
        "".into(),
    ));
}
pub(crate) struct Pending<'a> {
    pub(crate) router: &'a Router,
    pub(crate) state: &'a ServerState,
    pub(crate) ticket: Ticket,
}
impl Drop for Pending<'_> {
    fn drop(&mut self) {
        self.router.abort(&self.ticket);
        notify(self.state);
    }
}
pub(crate) async fn send(
    state: &ServerState,
    peer: &DPeer,
    channel_id: &str,
    key: &[u8],
    body: &str,
) -> Result<Value, String> {
    send_checked(state, peer, channel_id, key, body, || Ok(())).await
}
pub(crate) async fn send_checked(
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
        crate::services::peers::peer_address::current(&state.db, &peer.id, peer)
            .map_err(|e| e.to_string())?;
        if pending.ticket.transport == TransportType::Lan {
            let mut stop = state.stop.clone();
            let response = tokio::select! {_=stop.changed()=>return Err("Core stopped".into()),result=crate::services::peers::peer_lan::send(state,peer,channel_id,key,body,&validate)=>result};
            match response {
                Ok(response) => {
                    validate()?;
                    crate::services::peers::peer_address::current(&state.db, &peer.id, peer)
                        .map_err(|e| e.to_string())?;
                    router
                        .finish(&pending.ticket, Outcome::Connected)
                        .map_err(|e| e.to_string())?;
                    return Ok(response);
                }
                Err(crate::services::peers::peer_lan::Failure::Fatal(error)) => {
                    return Err(error);
                }
                Err(crate::services::peers::peer_lan::Failure::Unavailable(error)) => {
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
        let attempt = tokio::time::timeout(Duration::from_secs(15), async {
            match pending.ticket.transport {
                TransportType::Aware => {
                    crate::services::peers::peer_sdk::aware_send(state, peer, channel_id, key, body)
                        .await
                }
                TransportType::Ble => {
                    crate::services::peers::peer_sdk::ble_send(state, peer, channel_id, key, body)
                        .await
                }
                TransportType::Lan => unreachable!(),
            }
        })
        .await;
        let outcome = match attempt {
            Err(_) => return Err("SDK response timed out; delivery is unconfirmed".into()),
            Ok(Ok(response)) => {
                validate()?;
                crate::services::peers::peer_address::current(&state.db, &peer.id, peer)
                    .map_err(|e| e.to_string())?;
                router
                    .finish(&pending.ticket, Outcome::Connected)
                    .map_err(|e| e.to_string())?;
                return Ok(response);
            }
            Ok(Err(crate::services::peers::peer_lan::Failure::Fatal(error))) => {
                return Err(error);
            }
            Ok(Err(crate::services::peers::peer_lan::Failure::Unavailable(error))) => {
                Outcome::Unavailable { error }
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
pub(crate) enum Request {
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
    AwareConfig {
        id: String,
    },
}
pub(crate) async fn call(
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
                    &plain_rs::utils::base64::base64_decode(&key),
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
                json!({"peer":crate::services::peers::peer_address::view(&peer),"ticket":step.ticket,"error":step.error})
            }
            Request::FinishDownload { ticket, outcome } => {
                serde_json::to_value(state.transport.finish(&ticket, outcome)?)?
            }
            Request::AwareConfig { id } => {
                let peer=peers::get(&state.db,&id)?.ok_or_else(||anyhow::anyhow!("Unknown peer"))?;
                let key=plain_rs::utils::base64::base64_decode(&peer.key);
                anyhow::ensure!(key.is_empty() || key.len()==32,"Invalid Aware pairing key");
                let actor=state.prefs.get::<String>(plain_rs::prefs::system::keys::CLIENT_ID)?.filter(|s|!s.is_empty()).ok_or_else(||anyhow::anyhow!("Missing local identity"))?;
                json!({"pmk":if key.is_empty(){Value::Null}else{json!(plain_rs::utils::base64::base64_encode(&key))},"isClient":actor<id,"localPort":state.prefs.get_user::<u16>("https_port")?.unwrap_or(8443)})
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
#[path = "../../../tests/unit/services/peers/peer_transport.rs"]
mod tests;
