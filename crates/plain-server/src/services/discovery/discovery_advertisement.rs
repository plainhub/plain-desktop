use crate::assembly::server::ServerState;
use anyhow::Result;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use plain_rs::discovery::advertisement;
use plain_rs::discovery::advertisement::Facts;
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum Request {
    Reply {},
    Mdns {},
    Ble {},
    BleDecode { payload: Option<Vec<u8>> },
    BleShortId { id: String },
}
pub(crate) async fn execute(state: &ServerState, request: Request) -> Result<Value> {
    match &request {
        Request::BleDecode { payload } => {
            anyhow::ensure!(
                payload.as_ref().is_none_or(|bytes| bytes.len() <= 1650),
                "BLE payload exceeds advertising limit"
            );
            return Ok(serde_json::to_value(
                payload.as_deref().and_then(advertisement::decode_ble),
            )?);
        }
        Request::BleShortId { id } => {
            anyhow::ensure!(!id.is_empty() && id.len() <= 1024, "Invalid peer identity");
            return Ok(json!(plain_rs::discovery::nearby_wire::short_id(id)));
        }
        _ => {}
    }
    let facts: Facts = serde_json::from_value(
        state
            .host
            .call("discoveryFacts", json!({}))
            .await
            .map_err(anyhow::Error::msg)?,
    )?;
    let id = state
        .prefs
        .get::<String>(plain_rs::prefs::system::keys::CLIENT_ID)?
        .unwrap_or_default();
    if matches!(request, Request::Ble {}) {
        return Ok(json!(plain_rs::utils::base64::base64_encode(
            &advertisement::ble(&id, facts.aware_supported, facts.aware_running)?
        )));
    }
    let settings = plain_rs::prefs::user::snapshot(&state.prefs)?;
    let name = settings.device_name;
    let port = state.public_ports().await.map(|ports| ports.1).unwrap_or(0);
    let reply = advertisement::reply(&id, &name, port, facts)?;
    match request {
        Request::Reply {} => Ok(serde_json::to_value(reply)?),
        Request::Mdns {} => Ok(serde_json::to_value(advertisement::mdns(
            &reply,
            &plain_rs::prefs::ensure_mdns_hostname(&state.prefs),
        )?)?),
        Request::Ble {} | Request::BleDecode { .. } | Request::BleShortId { .. } => unreachable!(),
    }
}
#[cfg(test)]
#[path = "../../../tests/unit/services/discovery/discovery_advertisement.rs"]
mod tests;
