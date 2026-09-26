use serde::Serialize;

use crate::local::dlna::receiver_engine::DlnaEngine;
use crate::local::dlna::types::{DlnaMediaType, DlnaPlaybackState, PendingCastRequest};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DlnaStateSnapshot {
    pub enabled: bool,
    pub is_running: bool,
    pub is_retrying: bool,
    pub port: u16,
    pub media_uri: String,
    pub media_title: String,
    pub media_album_art_uri: String,
    pub media_type: DlnaMediaType,
    pub playback_state: DlnaPlaybackState,
    pub current_position_ms: i64,
    pub duration_ms: i64,
    pub pending_cast_request: Option<PendingCastRequest>,
    pub start_error: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DlnaSenders {
    pub allowed: Vec<String>,
    pub denied: Vec<String>,
}

#[tauri::command]
pub async fn dlna_state(
    prefs: tauri::State<'_, std::sync::Arc<crate::prefs::Prefs>>,
    engine: tauri::State<'_, DlnaEngine>,
) -> Result<DlnaStateSnapshot, String> {
    let s = engine.state.read().await;
    Ok(DlnaStateSnapshot {
        enabled: crate::prefs::dlna::enabled(&prefs),
        is_running: s.is_running,
        is_retrying: s.is_retrying,
        port: s.port,
        media_uri: s.media_uri.clone(),
        media_title: s.media_title.clone(),
        media_album_art_uri: s.media_album_art_uri.clone(),
        media_type: s.media_type,
        playback_state: s.playback_state,
        current_position_ms: s.current_position_ms,
        duration_ms: s.duration_ms,
        pending_cast_request: s.pending_cast_request.clone(),
        start_error: s.start_error.clone(),
    })
}

/// Persist the DLNA receiver toggle and start/stop the renderer engine.
/// Mirrors plain-app's `DlnaReceiverSection` toggle handler.
#[tauri::command]
pub async fn dlna_set_enabled(
    prefs: tauri::State<'_, std::sync::Arc<crate::prefs::Prefs>>,
    engine: tauri::State<'_, DlnaEngine>,
    server: tauri::State<'_, crate::local::server::LocalServerState>,
    enabled: bool,
) -> Result<(), String> {
    crate::prefs::dlna::set_enabled(&prefs, enabled);
    if enabled {
        engine.start(server.port()).await;
    } else {
        engine.stop().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn dlna_accept_cast(
    prefs: tauri::State<'_, std::sync::Arc<crate::prefs::Prefs>>,
    engine: tauri::State<'_, DlnaEngine>,
    remember: bool,
) -> Result<(), String> {
    engine.accept_cast(remember, &prefs).await;
    Ok(())
}

#[tauri::command]
pub async fn dlna_reject_cast(
    prefs: tauri::State<'_, std::sync::Arc<crate::prefs::Prefs>>,
    engine: tauri::State<'_, DlnaEngine>,
    remember: bool,
) -> Result<(), String> {
    engine.reject_cast(remember, &prefs).await;
    Ok(())
}

#[tauri::command]
pub fn dlna_senders(
    prefs: tauri::State<'_, std::sync::Arc<crate::prefs::Prefs>>,
) -> Result<DlnaSenders, String> {
    Ok(DlnaSenders {
        allowed: crate::prefs::dlna::senders(&prefs, "dlna_allowed_senders"),
        denied: crate::prefs::dlna::senders(&prefs, "dlna_denied_senders"),
    })
}

#[tauri::command]
pub fn dlna_remove_sender(
    prefs: tauri::State<'_, std::sync::Arc<crate::prefs::Prefs>>,
    kind: String,
    ip: String,
) -> Result<(), String> {
    let key = if kind == "allowed" {
        "dlna_allowed_senders"
    } else {
        "dlna_denied_senders"
    };
    crate::prefs::dlna::remove_sender(&prefs, key, &ip);
    Ok(())
}
