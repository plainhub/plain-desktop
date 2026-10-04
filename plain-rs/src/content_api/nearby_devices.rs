use super::server::ServerState;
use crate::{
    chat::{lan_ip::Interface, nearby_devices::Device},
    db::chat_store,
    ws_event::WsEvent,
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Request {
    Snapshot,
    Scanning {
        lan: bool,
        ble: bool,
    },
    Seen {
        device: Device,
        visible: bool,
        resident: bool,
    },
}
fn publish(state: &ServerState, event_id: Option<&str>) {
    let _ = state.events.send(WsEvent::broadcast(
        crate::chat::events::WS_NEARBY_DEVICE_FOUND,
        json!({"revision":state.nearby_devices.snapshot().revision,"eventId":event_id}).to_string(),
    ));
}
async fn execute(state: &ServerState, request: Request) -> anyhow::Result<serde_json::Value> {
    match request {
        Request::Snapshot => {}
        Request::Scanning { lan, ble } => {
            state
                .nearby_devices
                .scanning(lan, ble, chat_store::nearby::all(&state.db)?)?;
            publish(state, None);
        }
        Request::Seen {
            device,
            visible,
            resident,
        } => {
            if device.id
                == state
                    .prefs
                    .get::<String>("client_id")
                    .ok()
                    .flatten()
                    .unwrap_or_default()
            {
                return Ok(json!({"ignored":true}));
            }
            device.validate()?;
            if resident {
                chat_store::nearby::save(&state.db, &device.cache())?;
                let device_type = serde_json::from_value(json!(device.device_type))
                    .unwrap_or(crate::chat::enums::DeviceType::Other);
                chat_store::peers::discovered(
                    &state.db,
                    &device.id,
                    &device.ips,
                    device.port,
                    &device.name,
                    device_type,
                )?;
            }
            if visible
                || state
                    .nearby_devices
                    .snapshot()
                    .devices
                    .iter()
                    .any(|d| d.id == device.id)
            {
                let id = device.id.clone();
                let emit = state.nearby_devices.observe(device, visible)?;
                publish(state, (visible && emit).then_some(id.as_str()));
            }
        }
    }
    let paired: std::collections::HashSet<_> = chat_store::peers::all(&state.db)?
        .into_iter()
        .filter(|p| p.is_paired())
        .map(|p| p.id)
        .collect();
    let mut snapshot = serde_json::to_value(state.nearby_devices.snapshot())?;
    for device in snapshot["devices"].as_array_mut().unwrap() {
        device["status"] = json!(if paired.contains(device["id"].as_str().unwrap()) {
            "PAIRED"
        } else {
            "UNPAIRED"
        });
    }
    Ok(snapshot)
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
        Ok(result) => Json(json!({"result":result})).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":e.to_string()})),
        )
            .into_response(),
    }
}
async fn sweep(state: &ServerState) -> anyhow::Result<()> {
    let stale = state.nearby_devices.stale(&state.db)?;
    if stale.is_empty() {
        return Ok(());
    }
    let facts = state
        .host
        .call("nearbyScanFacts", json!({}))
        .await
        .map_err(anyhow::Error::msg)?;
    if facts.get("paused").and_then(serde_json::Value::as_bool) != Some(false) {
        return Ok(());
    }
    let interfaces: Vec<Interface> = serde_json::from_value(
        facts
            .get("interfaces")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Missing interfaces"))?,
    )?;
    let mut changed = false;
    for chunk in stale.chunks(4) {
        if !state.nearby_devices.active() {
            break;
        }
        let interfaces = &interfaces;
        let results = futures_util::future::join_all(chunk.iter().map(|p| async move {
            let ip = crate::chat::lan_ip::best(&p.device.ips, &interfaces);
            let alive = p.device.discovery_methods.iter().any(|v| v == "LAN")
                && !ip.is_empty()
                && crate::chat::nearby_http::probe(&ip, p.device.port)
                    .await
                    .unwrap_or(false);
            (p, alive)
        }))
        .await;
        for (probe, alive) in results {
            changed |= state.nearby_devices.verified(&state.db, probe, alive)?;
        }
    }
    if changed {
        publish(state, None);
    }
    Ok(())
}
pub(super) fn start(state: ServerState) {
    tokio::spawn(async move {
        let mut stop = state.stop.clone();
        let mut timer = tokio::time::interval(std::time::Duration::from_secs(20));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! { _=stop.changed()=>break, _=timer.tick()=>{tokio::select! {_=stop.changed()=>break, _=sweep(&state)=>{}}} }
        }
    });
}

#[cfg(all(test, feature = "http_transport"))]
#[path = "../../tests/unit/content_api/nearby_devices.rs"]
mod tests;
