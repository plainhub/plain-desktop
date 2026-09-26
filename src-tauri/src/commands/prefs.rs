//! IPC bridge for the frontend `lib/prefs.ts` — the Tauri replacement
//! for tauri-plugin-store. Backed by the process-wide `Arc<Prefs>`
//! (see `lib.rs` setup); reads hit the in-memory map, writes persist
//! atomically to `<app_data_dir>/prefs.json`.

use std::sync::Arc;

use plain_rs::prefs::Prefs;

#[tauri::command]
pub fn prefs_get_all(prefs: tauri::State<'_, Arc<Prefs>>) -> Vec<(String, serde_json::Value)> {
    prefs.entries()
}

#[tauri::command]
pub fn prefs_set(
    prefs: tauri::State<'_, Arc<Prefs>>,
    key: String,
    value: serde_json::Value,
) -> Result<(), String> {
    prefs
        .set(&key, value)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn prefs_remove(prefs: tauri::State<'_, Arc<Prefs>>, key: String) -> Result<(), String> {
    prefs.remove(&key).map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn prefs_clear(prefs: tauri::State<'_, Arc<Prefs>>) -> Result<(), String> {
    prefs.clear().map_err(|e| e.to_string())
}
