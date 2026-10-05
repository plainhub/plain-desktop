use super::{
    pairing::{self, Device},
    server::ServerState,
};
use crate::chat::{
    nearby_http::{self, Message},
    pairing::{
        protocol::{PairingCancel, PairingRequest, PairingResponse},
        sessions::{Target, Ticket},
    },
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};
struct Association {
    signature: String,
    address: String,
    received: Instant,
}
#[derive(Default)]
pub(super) struct Runtime(Mutex<HashMap<String, Association>>);
impl Runtime {
    fn check_address(&self, request: &PairingRequest, address: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            !address.is_empty() && address.len() <= 256,
            "Invalid BLE sender address"
        );
        let mut map = self.0.lock().unwrap();
        map.retain(|_, v| v.received.elapsed() < Duration::from_secs(300));
        anyhow::ensure!(
            map.contains_key(&request.from_id) || map.len() < 128,
            "Incoming BLE pairing capacity exceeded"
        );
        Ok(())
    }
    fn remember(&self, request: &PairingRequest, address: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            !address.is_empty() && address.len() <= 256,
            "Invalid BLE sender address"
        );
        let mut map = self.0.lock().unwrap();
        map.retain(|_, v| v.received.elapsed() < Duration::from_secs(300));
        anyhow::ensure!(
            map.contains_key(&request.from_id) || map.len() < 128,
            "Incoming BLE pairing capacity exceeded"
        );
        map.insert(
            request.from_id.clone(),
            Association {
                signature: request.signature.clone(),
                address: address.into(),
                received: Instant::now(),
            },
        );
        Ok(())
    }
    fn take(&self, id: &str, signature: &str) -> Option<String> {
        let mut map = self.0.lock().unwrap();
        if !map.get(id).is_some_and(|a| a.signature == signature) {
            return None;
        }
        map.remove(id)
            .filter(|a| a.received.elapsed() < Duration::from_secs(300))
            .map(|a| a.address)
    }
    fn forget(&self, id: &str) {
        self.0.lock().unwrap().remove(id);
    }
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Request {
    States,
    PendingRequest {
        id: String,
        signature: String,
    },
    Start {
        target: Target,
        ips: Vec<String>,
        methods: Vec<String>,
        ble: bool,
        device: Device,
    },
    StartBle {
        target: Target,
        device: Device,
    },
    StartLan {
        target: Target,
        ips: Vec<String>,
        device: Device,
    },
    Cancel {
        id: String,
        generation: Option<String>,
    },
    ReceiveRequest {
        request: PairingRequest,
        address: String,
        ble: bool,
    },
    ReceiveCancel {
        cancel: PairingCancel,
    },
    Complete {
        response: PairingResponse,
        sender_ip: String,
    },
    Respond {
        request: PairingRequest,
        accepted: bool,
        device: Device,
    },
}
pub(super) fn event(state: &ServerState, kind: i32, value: Value) {
    let _ = state
        .events
        .send(crate::ws_event::WsEvent::broadcast(kind, value.to_string()));
}
pub(super) fn failed(state: &ServerState, target: &Target, error: &str, generation: Option<&str>) {
    let mut result =
        json!({"deviceId":target.device_id,"deviceName":target.device_name,"error":error});
    if let Some(generation) = generation {
        result["generation"] = json!(generation);
    }
    event(state, crate::chat::events::WS_PAIRING_FAILED, result);
}
fn canceled(state: &ServerState, value: &Value) {
    let ticket = &value["ticket"];
    event(
        state,
        crate::chat::events::WS_PAIRING_CANCELLED,
        json!({"deviceId":ticket["deviceId"],"deviceName":ticket["deviceName"]}),
    );
}
pub(super) fn success(state: &ServerState, peer: &Value, ip: &str) {
    event(
        state,
        crate::chat::events::WS_PAIRING_SUCCESS,
        json!({"deviceId":peer["id"],"deviceName":peer["name"],"ip":ip,"key":peer["key"]}),
    );
}
async fn execute(state: &ServerState, request: Request) -> anyhow::Result<Value> {
    Ok(match request {
        Request::States=>json!(state.pairing.states().into_iter().map(|(ticket,phase)|json!({"deviceId":ticket.target.device_id,"generation":ticket.generation,"phase":phase})).collect::<Vec<_>>()),
        Request::PendingRequest { id, signature } => {
            json!(state.pairing.incoming_target(&id, &signature).is_some())
        }
        Request::Start {
            target,
            ips,
            methods,
            ble,
            device,
        } => {
            if methods.iter().any(|m| m == "LAN") && !ips.is_empty() {
                Box::pin(execute(
                    state,
                    Request::StartLan {
                        target,
                        ips,
                                    device,
                    },
                ))
                .await?
            } else if methods.iter().any(|m| m == "BLE") && ble {
                super::ble_pairing::start(state, target, device)?
            } else {
                failed(state, &target, "No pairing transport available", None);
                Value::Null
            }
        }
        Request::StartBle { target, device } => super::ble_pairing::start(state, target, device)?,
        Request::StartLan {
            mut target,
            ips,
            device,
        } => {
            target.device_ip = crate::chat::lan_ip::best(&ips, &crate::chat::lan_ip::local_interfaces());
            if target.device_ip.is_empty() {
                failed(state, &target, "No reachable pairing address", None);
                return Ok(Value::Null);
            }
            let result = pairing::start(&state.prefs, &state.pairing, target, device)?;
            let ticket: Ticket = serde_json::from_value(result["ticket"].clone())?;
            let request: PairingRequest = serde_json::from_value(result["request"].clone())?;
            let sent = nearby_http::send(
                &ticket.target.device_ip,
                ticket.target.device_port,
                &Message::PairRequest(request),
            )
            .await
            .unwrap_or(false);
            if sent {
                if state.pairing.mark_sent(&ticket.target.device_id,&ticket.generation)
                {
                    event(
                        state,
                        crate::chat::events::WS_PAIRING_STARTED,
                        json!({"deviceId":ticket.target.device_id,"deviceName":ticket.target.device_name,"generation":ticket.generation}),
                    );
                }
            } else if state
                .pairing
                .cancel(&ticket.target.device_id, Some(&ticket.generation))
                .is_some()
            {
                failed(
                    state,
                    &ticket.target,
                    "Failed to send pairing request",
                    Some(&ticket.generation),
                );
            }
            json!({"sent":sent,"ticket":ticket})
        }
        Request::Cancel { id, generation } => {
            let value = pairing::cancel(&state.prefs, &state.pairing, &id, generation.as_deref())?;
            if !value.is_null() {
                canceled(state, &value);
                let ticket: Ticket = serde_json::from_value(value["ticket"].clone())?;
                let cancel: PairingCancel = serde_json::from_value(value["cancel"].clone())?;
                if !ticket.target.device_ip.is_empty() {
                    let _ = nearby_http::send(
                        &ticket.target.device_ip,
                        ticket.target.device_port,
                        &Message::PairCancel(cancel),
                    )
                    .await;
                }
            }
            value
        }
        Request::ReceiveRequest {
            mut request,
            address,
            ble,
        } => {
            if !ble {
                request.from_ip = address.clone();
            }
            if !crate::chat::pairing::security::verify_request(&request) {
                return Ok(Value::Null);
            }
            if ble {
                state.pairing_runtime.check_address(&request, &address)?;
            }
            let is_new = pairing::receive_request(&state.pairing, &request).unwrap_or(false);
            if ble
                && state
                    .pairing
                    .incoming_target(&request.from_id, &request.signature)
                    .is_some()
            {
                if let Err(error) = state.pairing_runtime.remember(&request, &address) {
                    if is_new {
                        state
                            .pairing
                            .take_incoming(&request.from_id, &request.signature);
                    }
                    return Err(error);
                }
            }
            if is_new {
                event(
                    state,
                    crate::chat::events::WS_PAIRING_REQUEST_RECEIVED,
                    serde_json::to_value(&request)?,
                );
            }
            json!(is_new)
        }
        Request::ReceiveCancel { cancel } => {
            let id = cancel.from_id.clone();
            let result = pairing::receive_cancel(&state.prefs, &state.pairing, cancel)?;
            if !result.is_null() {
                state.pairing_runtime.forget(&id);
                event(
                    state,
                    crate::chat::events::WS_PAIRING_CANCELLED,
                    result.clone(),
                );
            }
            result
        }
        Request::Complete {
            response,
            sender_ip,
        } => {
            let result = pairing::complete(
                &state.db,
                &state.prefs,
                &state.pairing,
                response,
                &sender_ip,
            )?;
            if !result.is_null() {
                if result["peer"].is_null() {
                    let ticket: Ticket = serde_json::from_value(result["ticket"].clone())?;
                    failed(
                        state,
                        &ticket.target,
                        result["error"].as_str().unwrap_or("Pairing failed"),
                        Some(&ticket.generation),
                    );
                } else {
                    success(state, &result["peer"], &sender_ip);
                }
            }
            result
        }
        Request::Respond {
            request,
            accepted,
            device,
        } => {
            let target = state
                .pairing
                .incoming_target(&request.from_id, &request.signature);
            let result = pairing::respond(
                &state.db,
                &state.prefs,
                &state.pairing,
                request.clone(),
                accepted,
                device,
            )?;
            if result.is_null() {
                return Ok(Value::Null);
            }
            let response: PairingResponse = serde_json::from_value(result["response"].clone())?;
            let target = target.ok_or_else(|| anyhow::anyhow!("Incoming pairing disappeared"))?;
            if !result["peer"].is_null() {
                success(state, &result["peer"], &target.device_ip);
            } else {
                failed(state, &target, "Pairing request was rejected", None);
            }
            let address = state
                .pairing_runtime
                .take(&request.from_id, &request.signature);
            let lan_response = response.clone();
            let lan = async {
                if !target.device_ip.is_empty() {
                    let _ = nearby_http::send(
                        &target.device_ip,
                        target.device_port,
                        &Message::PairResponse(lan_response),
                    )
                    .await;
                }
            };
            let ble = async {
                if let Some(address) = address {
                    let body = Message::PairResponse(response).wire()?;
                    let sent = state
                        .host
                        .call(
                            "pairingNotification",
                            json!({"address":address,"body":body}),
                        )
                        .await;
                    if !matches!(sent, Ok(Value::Bool(true))) {
                        failed(
                            state,
                            &target,
                            "Failed to send pairing response via BLE",
                            None,
                        );
                    }
                }
                Ok::<_, anyhow::Error>(())
            };
            let (_, ble) = tokio::join!(lan, ble);
            ble?;
            result
        }
    })
}
pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match execute(&state, request).await {
        Ok(value) => Json(json!({"result":value})).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":error.to_string()})),
        )
            .into_response(),
    }
}
#[cfg(test)]
#[path = "../../tests/unit/content_api/pairing_runtime.rs"]
mod tests;
