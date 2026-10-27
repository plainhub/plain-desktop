use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tauri::{State, WebviewWindow, ipc::Channel};

struct LoginCallbacks {
    id: u64,
    requested: Channel<Value>,
    closed: Channel<Value>,
    snapshot: Channel<Value>,
}

fn send(channel: &Channel<Value>, params: &Value) -> bool {
    match channel.send(params.clone()) {
        Ok(()) => true,
        Err(error) => {
            log::warn!("Native host callback delivery failed: {error}");
            false
        }
    }
}

#[derive(Default)]
pub struct HostCallbacks {
    next_id: AtomicU64,
    login: Mutex<HashMap<String, LoginCallbacks>>,
    notifications: Mutex<HashMap<String, (u64, Channel<Value>)>>,
    http_status: Mutex<HashMap<String, (u64, Channel<Value>)>>,
    clipboard: Mutex<HashMap<String, (u64, Channel<Value>)>>,
    network: Mutex<HashMap<String, (u64, Channel<Value>)>>,
}

impl HostCallbacks {
    fn id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn remove_view(&self, label: &str) {
        self.login.lock().unwrap().remove(label);
        self.notifications.lock().unwrap().remove(label);
        self.http_status.lock().unwrap().remove(label);
        self.clipboard.lock().unwrap().remove(label);
        self.network.lock().unwrap().remove(label);
    }

    pub fn deliver(&self, method: &str, params: &Value) -> Result<(), String> {
        match method {
            "systemWebLoginRequested" | "systemWebLoginClosed" | "systemWebLoginSnapshot" => {
                self.login.lock().unwrap().retain(|_, callbacks| {
                    let channel = match method {
                        "systemWebLoginRequested" => &callbacks.requested,
                        "systemWebLoginClosed" => &callbacks.closed,
                        _ => &callbacks.snapshot,
                    };
                    send(channel, params)
                });
            }
            "peerNotificationsChanged" => {
                self.notifications
                    .lock()
                    .unwrap()
                    .retain(|_, (_, channel)| send(channel, params));
            }
            "networkInterfacesChanged" => {
                self.network.lock().unwrap().retain(|_, (_, channel)| send(channel, params));
            }
            "peerClipboardChanged" => {
                self.clipboard.lock().unwrap().retain(|_, (_, channel)| send(channel, params));
            }
            "httpServerStatusChanged" => {
                self.http_status
                    .lock()
                    .unwrap()
                    .retain(|_, (_, channel)| send(channel, params));
            }
            _ => return Err(format!("Unsupported host callback: {method}")),
        }
        Ok(())
    }
}

#[tauri::command]
pub fn native_register_login_callbacks(
    window: WebviewWindow,
    callbacks: State<'_, HostCallbacks>,
    requested: Channel<Value>,
    closed: Channel<Value>,
    snapshot: Channel<Value>,
) -> u64 {
    let id = callbacks.id();
    callbacks.login.lock().unwrap().insert(
        window.label().into(),
        LoginCallbacks {
            id,
            requested,
            closed,
            snapshot,
        },
    );
    id
}

#[tauri::command]
pub fn native_unregister_login_callbacks(
    window: WebviewWindow,
    callbacks: State<'_, HostCallbacks>,
    id: u64,
) {
    let mut active = callbacks.login.lock().unwrap();
    if active
        .get(window.label())
        .is_some_and(|entry| entry.id == id)
    {
        active.remove(window.label());
    }
}

#[tauri::command]
pub fn native_register_peer_notifications(
    window: WebviewWindow,
    callbacks: State<'_, HostCallbacks>,
    channel: Channel<Value>,
) -> u64 {
    let id = callbacks.id();
    callbacks
        .notifications
        .lock()
        .unwrap()
        .insert(window.label().into(), (id, channel));
    id
}

#[tauri::command]
pub fn native_unregister_peer_notifications(
    window: WebviewWindow,
    callbacks: State<'_, HostCallbacks>,
    id: u64,
) {
    let mut active = callbacks.notifications.lock().unwrap();
    if active
        .get(window.label())
        .is_some_and(|entry| entry.0 == id)
    {
        active.remove(window.label());
    }
}

#[tauri::command]
pub fn native_register_http_status(
    window: WebviewWindow,
    callbacks: State<'_, HostCallbacks>,
    channel: Channel<Value>,
) -> u64 {
    let id = callbacks.id();
    callbacks
        .http_status
        .lock()
        .unwrap()
        .insert(window.label().into(), (id, channel));
    id
}

#[tauri::command]
pub fn native_unregister_http_status(
    window: WebviewWindow,
    callbacks: State<'_, HostCallbacks>,
    id: u64,
) {
    let mut active = callbacks.http_status.lock().unwrap();
    if active
        .get(window.label())
        .is_some_and(|entry| entry.0 == id)
    {
        active.remove(window.label());
    }
}

#[tauri::command]
pub fn native_register_peer_clipboard(
    window: WebviewWindow,
    callbacks: State<'_, HostCallbacks>,
    channel: Channel<Value>,
) -> u64 {
    let id = callbacks.id();
    callbacks.clipboard.lock().unwrap().insert(window.label().into(), (id, channel));
    id
}

#[tauri::command]
pub fn native_unregister_peer_clipboard(
    window: WebviewWindow,
    callbacks: State<'_, HostCallbacks>,
    id: u64,
) {
    let mut active = callbacks.clipboard.lock().unwrap();
    if active.get(window.label()).is_some_and(|entry| entry.0 == id) {
        active.remove(window.label());
    }
}

#[tauri::command]
pub fn native_register_network_changes(
    window: WebviewWindow,
    callbacks: State<'_, HostCallbacks>,
    channel: Channel<Value>,
) -> u64 {
    let id = callbacks.id();
    callbacks.network.lock().unwrap().insert(window.label().into(), (id, channel));
    id
}

#[tauri::command]
pub fn native_unregister_network_changes(
    window: WebviewWindow,
    callbacks: State<'_, HostCallbacks>,
    id: u64,
) {
    let mut active = callbacks.network.lock().unwrap();
    if active.get(window.label()).is_some_and(|entry| entry.0 == id) {
        active.remove(window.label());
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/commands/native/callbacks.rs"]
mod tests;
