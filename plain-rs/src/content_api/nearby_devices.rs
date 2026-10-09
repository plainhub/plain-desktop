use super::server::ServerState;
use crate::{chat::nearby_devices::Device, db::chat_store, ws_event::WsEvent};
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
    BleScanning {
        ble: bool,
    },
    Seen {
        device: Device,
        visible: bool,
        resident: bool,
    },
}
pub(super) const EVENT_UPDATED: i32 = 10011;

fn publish(state: &ServerState) {
    let _ = state.events.send(WsEvent::broadcast(
        EVENT_UPDATED,
        json!({"revision":state.nearby_devices.snapshot().revision}).to_string(),
    ));
}
#[derive(Clone)]
pub(super) struct Context {
    pub db: std::sync::Arc<crate::db::Db>,
    pub prefs: std::sync::Arc<crate::prefs::Prefs>,
    pub devices: std::sync::Arc<crate::chat::nearby_devices::Devices>,
    pub events: tokio::sync::broadcast::Sender<WsEvent>,
}
impl From<&ServerState> for Context {
    fn from(state: &ServerState) -> Self {
        Self {
            db: state.db.clone(),
            prefs: state.prefs.clone(),
            devices: state.nearby_devices.clone(),
            events: state.events.clone(),
        }
    }
}
impl Context {
    pub(super) fn seen(
        &self,
        device: Device,
        visible: bool,
        resident: bool,
    ) -> anyhow::Result<bool> {
        if device.id == self.prefs.get::<String>("client_id")?.unwrap_or_default() {
            return Ok(false);
        }
        device.validate()?;
        if resident {
            chat_store::nearby::save(&self.db, &device.cache())?;
            let kind = serde_json::from_value(json!(device.device_type))
                .unwrap_or(crate::chat::enums::DeviceType::Other);
            chat_store::peers::discovered(
                &self.db,
                &device.id,
                &device.ips,
                device.port,
                &device.name,
                kind,
            )?;
        }
        if visible
            || self
                .devices
                .snapshot()
                .devices
                .iter()
                .any(|d| d.id == device.id)
        {
            let id = device.id.clone();
            let emit = self.devices.observe(device, visible)?;
            let _ = self.events.send(WsEvent::broadcast(
                EVENT_UPDATED,
                json!({"revision":self.devices.snapshot().revision}).to_string(),
            ));
            if visible && emit {
                let snapshot = snapshot(&self.db, &self.devices)?;
                if let Some(device) = snapshot["devices"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|device| device["id"] == id)
                {
                    let _ = self.events.send(WsEvent::broadcast(
                        crate::chat::events::WS_NEARBY_DEVICE_FOUND,
                        device.to_string(),
                    ));
                }
            }
        }
        Ok(true)
    }
}
async fn execute(state: &ServerState, request: Request) -> anyhow::Result<serde_json::Value> {
    match request {
        Request::Snapshot => {}
        Request::BleScanning { ble } => {
            state.nearby_devices.scanning_modes(
                None,
                Some(ble),
                chat_store::nearby::all(&state.db)?,
            )?;
            publish(state);
        }
        Request::Seen {
            device,
            visible,
            resident,
        } => {
            if !Context::from(state).seen(device, visible, resident)? {
                return Ok(json!({"ignored":true}));
            }
        }
    }
    snapshot(&state.db, &state.nearby_devices)
}
fn snapshot(
    db: &crate::db::Db,
    devices: &crate::chat::nearby_devices::Devices,
) -> anyhow::Result<serde_json::Value> {
    let paired: std::collections::HashSet<_> = chat_store::peers::all(db)?
        .into_iter()
        .filter(|p| p.is_paired())
        .map(|p| p.id)
        .collect();
    let mut snapshot = serde_json::to_value(devices.snapshot())?;
    let interfaces = crate::chat::lan_ip::local_interfaces();
    for device in snapshot["devices"].as_array_mut().unwrap() {
        let ips: Vec<String> = serde_json::from_value(device["ips"].clone())?;
        device["bestIp"] = json!(crate::chat::lan_ip::best(&ips, &interfaces));
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
    let interfaces = crate::chat::lan_ip::local_interfaces();
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
        publish(state);
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
