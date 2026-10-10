use super::server::ServerState;
use crate::dlna_sender::types::DiscoveredDevice;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

pub(super) const EVENT_UPDATED: &'static str = "DLNA_SENDER_UPDATED";
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Device {
    pub id: String,
    pub host_address: String,
    pub name: String,
    pub location: String,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Item {
    pub path: String,
    pub title: String,
    #[serde(default)]
    pub album_art: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub duration_ms: i64,
    #[serde(default)]
    pub audio: bool,
}
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Snapshot {
    pub version: u64,
    #[serde(skip)]
    pub current_audio: bool,
    pub devices: Vec<Device>,
    pub current_device: Option<Device>,
    pub items: Vec<Item>,
    pub current_uri: String,
    pub playing: bool,
    pub progress_ms: i64,
    pub duration_ms: i64,
    pub supports_callback: bool,
    pub active: bool,
    pub sid: String,
}
#[derive(Default)]
pub(super) struct Runtime {
    pub snapshot: Mutex<Snapshot>,
    pub devices: Mutex<HashMap<String, DiscoveredDevice>>,
    pub scan: Mutex<Option<tokio::task::JoinHandle<()>>>,
    pub polling: Mutex<Option<tokio::task::JoinHandle<()>>>,
    pub renewal: Mutex<Option<tokio::task::JoinHandle<()>>>,
    pub scan_lease: Mutex<Option<String>>,
    pub receiver_lease: Mutex<Option<String>>,
    pub subscription_device: Mutex<Option<DiscoveredDevice>>,
    pub subscription_renew_after: Mutex<std::time::Duration>,
    pub callback_sequence: Mutex<Option<u32>>,
    pub receiver_operations: tokio::sync::Mutex<()>,
    pub operations: tokio::sync::Mutex<()>,
}
impl Runtime {
    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().unwrap().clone()
    }
    pub fn publish(&self, state: &ServerState) {
        let payload = {
            let mut snapshot = self.snapshot.lock().unwrap();
            snapshot.version = snapshot.version.saturating_add(1);
            serde_json::to_string(&*snapshot).unwrap()
        };
        let _ = state
            .events
            .send(crate::ws_event::WsEvent::broadcast(EVENT_UPDATED, payload));
    }
    pub fn stop_tasks(&self) {
        for slot in [&self.scan, &self.polling, &self.renewal] {
            if let Some(task) = slot.lock().unwrap().take() {
                task.abort();
            }
        }
    }
    pub fn device(&self) -> Result<DiscoveredDevice, String> {
        let id = self
            .snapshot()
            .current_device
            .ok_or("No cast device selected")?
            .id;
        self.devices
            .lock()
            .unwrap()
            .get(&id)
            .cloned()
            .ok_or_else(|| "Cast device disappeared".into())
    }
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Command {
    Snapshot {},
    StartScan,
    StopScan,
    Select {
        id: String,
    },
    Cast {
        item: Item,
    },
    Play,
    Pause,
    Seek {
        #[serde(rename = "positionMs")]
        position_ms: i64,
    },
    Stop,
    Exit,
    Add {
        item: Item,
    },
    Remove {
        path: String,
    },
    RemoveAt {
        index: usize,
    },
    Reorder {
        from: usize,
        to: usize,
    },
    Clear,
}
pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<Command>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let runtime = state.cast.clone();
    let _guard = runtime.operations.lock().await;
    let result = execute(&state, &runtime, command).await;
    runtime.publish(&state);
    match result {
        Ok(()) => Json(json!({"result":runtime.snapshot()})).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":error,"result":runtime.snapshot()})),
        )
            .into_response(),
    }
}
async fn execute(
    state: &ServerState,
    runtime: &Arc<Runtime>,
    command: Command,
) -> Result<(), String> {
    use Command::*;
    match command {
        Snapshot {} => {}
        StartScan => {
            if runtime
                .scan
                .lock()
                .unwrap()
                .as_ref()
                .is_none_or(|task| task.is_finished())
            {
                release_permission(state, &runtime.scan_lease).await;
                acquire_permission(state, &runtime.scan_lease).await?;
                super::dlna_sender_scan::start(state);
            }
        }
        StopScan => {
            if let Some(task) = runtime.scan.lock().unwrap().take() {
                task.abort();
            }
            release_permission(state, &runtime.scan_lease).await;
        }
        Select { id } => {
            let device = runtime
                .snapshot()
                .devices
                .into_iter()
                .find(|d| d.id == id)
                .ok_or("Unknown cast device")?;
            if runtime
                .snapshot()
                .current_device
                .as_ref()
                .is_some_and(|current| current.id != id)
            {
                super::dlna_sender_playback::end(state, false).await;
            }
            runtime.snapshot.lock().unwrap().current_device = Some(device);
        }
        Cast { item } => super::dlna_sender_playback::cast(state, item, false).await?,
        Play | Pause => {
            let play = matches!(command, Play);
            let device = runtime.device()?;
            super::dlna_sender_playback::soap(
                state,
                &device,
                if play { "Play" } else { "Pause" },
                if play {
                    "<InstanceID>0</InstanceID><Speed>1</Speed>"
                } else {
                    "<InstanceID>0</InstanceID>"
                },
            )
            .await?;
            runtime.snapshot.lock().unwrap().playing = play;
        }
        Seek { position_ms } => {
            let device = runtime.device()?;
            let ms = position_ms.max(0);
            let seconds = ms / 1000;
            let target = format!(
                "{:02}:{:02}:{:02}",
                seconds / 3600,
                seconds / 60 % 60,
                seconds % 60
            );
            super::dlna_sender_playback::soap(
                state,
                &device,
                "Seek",
                &format!(
                    "<InstanceID>0</InstanceID><Unit>REL_TIME</Unit><Target>{target}</Target>"
                ),
            )
            .await?;
            runtime.snapshot.lock().unwrap().progress_ms = ms;
        }
        Stop | Exit => {
            let stop = matches!(command, Stop);
            super::dlna_sender_playback::end(state, stop).await;
        }
        Add { item } => runtime.snapshot.lock().unwrap().items.push(item),
        Remove { path } => runtime
            .snapshot
            .lock()
            .unwrap()
            .items
            .retain(|item| item.path != path),
        RemoveAt { index } => {
            let mut s = runtime.snapshot.lock().unwrap();
            if index < s.items.len() {
                s.items.remove(index);
            }
        }
        Reorder { from, to } => {
            let mut s = runtime.snapshot.lock().unwrap();
            if from < s.items.len() && to < s.items.len() {
                let item = s.items.remove(from);
                s.items.insert(to, item);
            }
        }
        Clear => {
            let mut s = runtime.snapshot.lock().unwrap();
            s.items.clear();
            s.current_uri.clear();
            s.current_audio = false;
            s.playing = false;
            s.progress_ms = 0;
            s.duration_ms = 0;
            s.supports_callback = false;
        }
    }
    Ok(())
}

pub(super) async fn acquire_permission(
    state: &ServerState,
    slot: &Mutex<Option<String>>,
) -> Result<(), String> {
    if slot.lock().unwrap().is_some() {
        return Ok(());
    }
    let lease = uuid::Uuid::new_v4().to_string();
    if state
        .host
        .call("mdnsMulticast", json!({"lease":lease,"acquire":true}))
        .await?
        .as_bool()
        != Some(true)
    {
        return Err("Multicast permission unavailable".into());
    }
    *slot.lock().unwrap() = Some(lease);
    Ok(())
}
pub(super) async fn release_permission(state: &ServerState, slot: &Mutex<Option<String>>) {
    let lease = slot.lock().unwrap().take();
    if let Some(lease) = lease {
        let _ = state
            .host
            .call("mdnsMulticast", json!({"lease":lease,"acquire":false}))
            .await;
    }
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/dlna_sender_runtime.rs"]
mod tests;
