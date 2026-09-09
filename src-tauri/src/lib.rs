mod commands;
mod http_proxy;
mod local;
mod prefs;
mod utils;

use std::sync::Arc;
use tauri::Manager;

const _: &[u8] = include_bytes!("../icons/icon.icns");

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        let windows = app.webview_windows();
        let candidate = select_reopen_window_label(
            windows
                .iter()
                .map(|(label, window)| (label.as_str(), window.is_visible().ok())),
        );
        if let Some(window) = candidate.and_then(|label| windows.get(&label)) {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    }));
    let builder =
        if let Some(plugin) = commands::screen_capture::shortcut::ordinary_shortcut_plugin() {
            builder.plugin(plugin)
        } else {
            builder
        };
    let app = builder
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(if cfg!(debug_assertions) {
                    log::LevelFilter::Debug
                } else {
                    log::LevelFilter::Info
                })
                .level_for("tungstenite", log::LevelFilter::Warn)
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("plain".to_string()),
                    }),
                ])
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(commands::HttpClient::new())
        .manage(commands::media_preview_pool::MediaPreviewState::default())
        .manage(commands::screen_capture::runtime::ScreenCaptureRuntime::default())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            commands::macos_menu::setup(app)?;

            #[cfg(target_os = "linux")]
            match commands::screen_capture::shortcut::current_linux_shortcut_backend() {
                commands::screen_capture::shortcut::LinuxShortcutBackend::OrdinaryPlugin => {
                    if let Err(error) =
                        commands::screen_capture::shortcut::register_ordinary_capture_shortcut(
                            app.handle(),
                        )
                    {
                        log::warn!("screen capture shortcut registration failed: {error}");
                    }
                }
                commands::screen_capture::shortcut::LinuxShortcutBackend::WaylandPortalRequired => {
                    let handle = app.handle().clone();
                    tauri::async_runtime::spawn(async move {
                        match commands::screen_capture::shortcut::register_wayland_portal_capture_shortcut(
                            handle.clone(),
                        )
                        .await
                        {
                            Ok(guard) => {
                                if !handle.manage(guard) {
                                    log::warn!("Wayland capture shortcut guard was already installed");
                                }
                            }
                            Err(error) => {
                                log::warn!("Wayland capture shortcut registration failed: {error}");
                            }
                        }
                    });
                }
            }

            #[cfg(not(target_os = "linux"))]
            if let Err(error) =
                commands::screen_capture::shortcut::register_ordinary_capture_shortcut(app.handle())
            {
                log::warn!("screen capture shortcut registration failed: {error}");
            }

            #[cfg(not(target_os = "windows"))]
            {
                let runtime = app
                    .handle()
                    .state::<commands::screen_capture::runtime::ScreenCaptureRuntime>();
                let windows = commands::screen_capture::window::TauriCaptureWindowPort::new(
                    app.handle().clone(),
                );
                log::info!("screen capture overlay prewarm started");
                match runtime.ensure_overlay_native(&windows) {
                    Ok(init) => log::info!(
                        "screen capture overlay prewarm completed generation={}",
                        init.overlay_generation
                    ),
                    Err(error) => {
                        log::warn!("screen capture overlay prewarm failed: {error}");
                    }
                }
            }

            app.handle().manage(http_proxy::HttpProxyState::start());
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            let log_dir = app
                .path()
                .app_log_dir()
                .unwrap_or_else(|_| data_dir.join("logs"));
            let db_path = data_dir.join("local_chat.db");
            let db = match local::db::ChatDb::open(&db_path) {
                Ok(d) => Arc::new(d),
                Err(e) => panic!("local_db open failed: {e}"),
            };
            // Ensure persistent device identity once at startup.
            let handle = app.handle().clone();
            let identity = Arc::new(crate::prefs::ensure_identity(&handle));
            let device_name = Arc::new(std::sync::RwLock::new(identity.device_name.clone()));
            let mdns_hostname = Arc::new(std::sync::RwLock::new(
                crate::prefs::ensure_mdns_hostname(&handle),
            ));
            let peer_status = commands::discover::PeerStatusManager::new(db.clone(), identity.clone());
            let pairing_mgr = local::pairing::PairingManager::new(db.clone(), identity.clone());
            app.handle().manage(pairing_mgr.clone());
            let dlna_engine = Arc::new(local::dlna::receiver_engine::DlnaEngine::new());
            let app_version = app.package_info().version.to_string();
            let discover_mgr = commands::discover::NearbyDiscoverManager::new(
                db.clone(),
                identity.clone(),
                device_name.clone(),
                mdns_hostname,
                pairing_mgr.clone(),
                peer_status.clone(),
                0,
                app_version,
            );
            let local_server_state = local::server::LocalServerState::start(
                data_dir,
                log_dir,
                db.clone(),
                handle,
                identity.clone(),
                device_name.clone(),
                peer_status.clone(),
                discover_mgr.clone(),
                pairing_mgr.clone(),
                dlna_engine.clone(),
            );
            app.handle().manage(dlna_engine.clone());
            // Start the DLNA renderer at startup when the toggle is on.
            if crate::prefs::get_dlna_enabled(app.handle()) {
                let engine = dlna_engine.clone();
                let port = local_server_state.port();
                tauri::async_runtime::spawn(async move {
                    engine.start(port).await;
                });
            }
            peer_status.set_event_tx(local_server_state.event_tx.clone());
            discover_mgr.set_event_tx(local_server_state.event_tx.clone());
            discover_mgr.set_app_handle(app.handle().clone());
            discover_mgr.start();
            peer_status.set_discover_manager(discover_mgr.clone());
            peer_status.start();
            // Bridge pairing broadcast → both Tauri "pairing-event" and the
            // local-server WS `WS_PAIRING_*` events. The same event object
            // fans out to both transports so desktop (Tauri) and browser-only
            // (GraphQL/WebSocket) clients see identical pairing state.
            {
                let mut pairing_rx = pairing_mgr.subscribe();
                let app_handle = app.handle().clone();
                let ws_event_tx = local_server_state.event_tx.clone();
                tauri::async_runtime::spawn(async move {
                    use tauri::Emitter;
                    while let Ok(ev) = pairing_rx.recv().await {
                        log::info!(
                            "pairing_bridge: received PairingEvent kind={} device_id={}",
                            match &ev.kind {
                                crate::local::pairing::PairingEventKind::IncomingRequest { .. } => "IncomingRequest",
                                crate::local::pairing::PairingEventKind::Started => "Started",
                                crate::local::pairing::PairingEventKind::Success => "Success",
                                crate::local::pairing::PairingEventKind::Failed { .. } => "Failed",
                                crate::local::pairing::PairingEventKind::Cancelled => "Cancelled",
                            },
                            ev.device_id
                        );
                        let _ = app_handle.emit("pairing-event", ev.clone());
                        forward_pairing_event_to_ws(&ws_event_tx, &ev);
                    }
                });
            }
            app.handle().manage(discover_mgr);
            app.handle().manage(local_server_state);
            Ok(())
        })
        .on_page_load(|webview, payload| {
            let app = webview.app_handle().clone();
            let runtime = app.state::<commands::screen_capture::runtime::ScreenCaptureRuntime>();
            let windows = commands::screen_capture::window::TauriCaptureWindowPort::new(app.clone());
            let overlay_generation =
                commands::screen_capture::runtime::is_overlay_window_label(webview.label())
                    .then(|| {
                        payload.url().query_pairs().find_map(|(key, value)| {
                            (key == "overlayGeneration")
                                .then(|| value.parse::<u64>().ok())
                                .flatten()
                        })
                    })
                    .flatten();
            let result = match payload.event() {
                tauri::webview::PageLoadEvent::Started => {
                    if commands::screen_capture::runtime::is_overlay_window_label(webview.label()) {
                        log::info!(
                            "screen capture overlay page load started generation={overlay_generation:?}"
                        );
                    }
                    runtime.window_page_load_started(
                        webview.label(),
                        overlay_generation,
                        &windows,
                    )
                }
                tauri::webview::PageLoadEvent::Finished => {
                    if commands::screen_capture::runtime::is_overlay_window_label(webview.label()) {
                        log::info!(
                            "screen capture overlay page load finished generation={overlay_generation:?}"
                        );
                    }
                    runtime.overlay_page_load_finished(
                        webview.label(),
                        overlay_generation,
                        &windows,
                    )
                }
            };
            if let Err(error) = result {
                log::warn!("screen capture overlay page lifecycle cleanup failed: {error}");
            }
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                commands::screen_capture::commands::on_window_close_requested(
                    window.app_handle(),
                    window.label(),
                );
            }
            // Remember the frame while the window is still alive so the
            // dock-icon reopen can put it back exactly where it was.
            #[cfg(target_os = "macos")]
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                commands::window::remember_main_window_frame(
                    window.app_handle(),
                    window.label(),
                );
            }
            if let tauri::WindowEvent::Destroyed = event {
                commands::screen_capture::commands::on_window_destroyed(
                    window.app_handle(),
                    window.label(),
                );
                #[cfg(target_os = "macos")]
                commands::macos_dock::remove_window_device_name(window.label());
                // Any preview window dying (warm or visible) means we no
                // longer have a ready window. Rebuild so the next click is
                // fast. The user explicitly asked to let the close path
                // destroy the window — we don't intercept.
                commands::media_preview_pool::on_window_destroyed(
                    &window.app_handle().clone(),
                    window.label(),
                );
                // Windows/Linux follow the platform convention that closing
                // the last visible window quits the process; without this the
                // hidden media-preview warm window keeps closed instances
                // alive holding the local-server ports (the Windows zombie
                // pile-up). macOS keeps the standard behavior instead: the
                // app stays in the dock and windows reopen via the dock
                // menu's "New Window" — closing a window never exits.
                #[cfg(any(target_os = "windows", target_os = "linux"))]
                {
                    let destroyed_label = window.label();
                    if should_check_app_exit_after_destroy(destroyed_label) {
                        let app = window.app_handle();
                        let remaining = app
                            .webview_windows()
                            .into_iter()
                            .filter(|(label, _)| {
                                label != destroyed_label && keeps_process_alive(label)
                            });
                        if !any_window_visible(remaining.map(|(_, w)| w.is_visible().ok())) {
                            app.exit(0);
                        }
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::discover::login_peer,
            commands::discover::peer_address,
            commands::discover::logout_peer,
            commands::discover::list_login_peers,
            commands::discover::update_peer_name,
            commands::discover::mdns_snapshot,
            commands::discover::mdns_browse,
            commands::discover::mdns_start_browse,
            commands::discover::mdns_stop_browse,
            commands::discover::mdns_get_hostname,
            commands::discover::mdns_set_hostname,
            commands::discover::mdns_firewall_status,
            commands::discover::fix_mdns_firewall,
            commands::http_client::http_request,
            commands::ws_proxy::ws_start_proxy,
            commands::notification::send_macos_notification,
            commands::updater::check_for_updates,
            #[cfg(target_os = "macos")]
            commands::macos_menu::set_menu_locale,
            commands::window::open_window,
            commands::window::open_about_window,
            commands::window::set_window_device_name,
            commands::media_preview_pool::media_preview_init,
            commands::media_preview_pool::media_preview_activate,
            commands::reveal::reveal_chat_file,
            commands::reveal::save_chat_file_as,
            commands::reveal::save_text_file_as,
            commands::reveal::copy_chat_file_to_clipboard,
            commands::screen_capture::commands::screen_capture_register_target,
            commands::screen_capture::commands::screen_capture_unregister_target,
            commands::screen_capture::commands::screen_capture_start,
            commands::screen_capture::commands::screen_capture_ready,
            commands::screen_capture::commands::screen_capture_pending_frame,
            commands::screen_capture::commands::screen_capture_take_frame,
            commands::screen_capture::commands::screen_capture_frame_presented,
            commands::screen_capture::commands::screen_capture_overlay_work_area,
            commands::screen_capture::commands::screen_capture_submit_result,
            commands::screen_capture::commands::screen_capture_send_result,
            commands::screen_capture::commands::screen_capture_take_result,
            commands::screen_capture::commands::screen_capture_release_result,
            commands::screen_capture::commands::screen_capture_ack_result,
            commands::screen_capture::commands::screen_capture_save_result,
            commands::screen_capture::commands::screen_capture_copy_result,
            commands::screen_capture::commands::screen_capture_discard_result,
            commands::screen_capture::commands::screen_capture_open_permission_settings,
            commands::screen_capture::commands::screen_capture_report_client_error,
            commands::screen_capture::commands::screen_capture_report_bootstrap_error,
            commands::screen_capture::commands::screen_capture_invalidate_target,
            commands::screen_capture::commands::screen_capture_fail,
            commands::screen_capture::commands::screen_capture_cancel,
            commands::screen_capture::commands::screen_capture_unavailable,
            http_proxy::http_proxy_port,
            local::server::local_server_port,
            local::server::local_server_https_port,
            local::server::local_server_token,
            local::server::local_ipv4_strs,
            local::server::set_http_port,
            local::server::set_https_port,
            local::server::restart_server,
            local::pairing::commands::pair_device,
            local::pairing::commands::respond_pair_device,
            local::pairing::commands::cancel_pair_device,
            local::pairing::commands::get_device_identity,
            local::pairing::commands::set_device_name,
            local::dlna::commands::dlna_state,
            local::dlna::commands::dlna_set_enabled,
            local::dlna::commands::dlna_accept_cast,
            local::dlna::commands::dlna_reject_cast,
            local::dlna::commands::dlna_senders,
            local::dlna::commands::dlna_remove_sender,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");
    app.run(handle_run_event);
}

/// macOS dock-icon click with no visible window reopens the main window —
/// the standard `applicationShouldHandleReopen` behavior. With visible
/// windows the default activation already brings the app forward.
#[cfg(target_os = "macos")]
fn handle_run_event(app: &tauri::AppHandle, event: tauri::RunEvent) {
    if let tauri::RunEvent::Reopen {
        has_visible_windows: false,
        ..
    } = event
    {
        commands::window::reopen_main_window(app);
    }
}

#[cfg(not(target_os = "macos"))]
fn handle_run_event(_app: &tauri::AppHandle, _event: tauri::RunEvent) {}

/// Whether any window is (or may be) visible — an `Err` from `is_visible`
/// counts as visible so a flaky query can never exit a live app.
/// Only consulted on Windows/Linux; macOS never exits on window close.
#[cfg_attr(not(any(target_os = "windows", target_os = "linux")), allow(dead_code))]
fn any_window_visible(mut visibilities: impl Iterator<Item = Option<bool>>) -> bool {
    visibilities.any(|v| v.unwrap_or(true))
}

/// Capture sessions intentionally destroy their ephemeral utility webview.
/// That internal lifecycle event must never be interpreted as the user
/// closing Plain's final application window. Media-preview windows retain
/// their existing last-visible-window behavior.
#[cfg_attr(
    not(any(target_os = "windows", target_os = "linux", test)),
    allow(dead_code)
)]
fn should_check_app_exit_after_destroy(label: &str) -> bool {
    !commands::screen_capture::runtime::is_overlay_window_label(label)
}

/// A Windows capture bootstrap is intentionally mapped as an opaque,
/// click-through one-pixel anchor so WebView2 can initialize reliably. It is
/// infrastructure, not user-visible application state, and must not turn a
/// closed Plain instance into a background zombie.
#[cfg_attr(
    not(any(target_os = "windows", target_os = "linux", test)),
    allow(dead_code)
)]
fn keeps_process_alive(label: &str) -> bool {
    !commands::screen_capture::runtime::is_overlay_window_label(label)
}

/// Choose the regular application window that a second launch should reveal.
/// Hidden dynamic windows remain valid recovery targets after a failed capture;
/// utility and capture-overlay windows must never be surfaced as the main UI.
#[cfg_attr(
    not(any(target_os = "windows", target_os = "linux", test)),
    allow(dead_code)
)]
fn select_reopen_window_label<I, S>(windows: I) -> Option<String>
where
    I: IntoIterator<Item = (S, Option<bool>)>,
    S: AsRef<str>,
{
    let mut visible_dynamic = None;
    let mut hidden_dynamic = None;
    for (label, visibility) in windows {
        let label = label.as_ref();
        if !commands::screen_capture::runtime::is_regular_window_label(label) {
            continue;
        }
        if label == "main" {
            return Some(label.to_string());
        }
        if visibility.unwrap_or(true) {
            visible_dynamic.get_or_insert_with(|| label.to_string());
        } else {
            hidden_dynamic.get_or_insert_with(|| label.to_string());
        }
    }
    visible_dynamic.or(hidden_dynamic)
}

/// Wire-format struct mirroring plain-app's `DPairingResult`
/// (`app/src/main/java/com/ismartcoding/plain/data/DNearbyPair.kt`).
/// Sent over the WebSocket for `PAIRING_SUCCESS` / `PAIRING_FAILED` /
/// `PAIRING_CANCELED` so the browser sees a single flat shape regardless of
/// whether the WebSocket is served by plain-web's local Rust server or
/// plain-app's Android HTTP server.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DPairingResult<'a> {
    device_id: &'a str,
    device_name: &'a str,
    error: &'a str,
}

/// Translate a `PairingEvent` into the appropriate `WsEvent` and push it
/// to the local GraphQL WebSocket. Mirrors the event-type constants used
/// by the browser-side `app-socket.ts` (`pairing_request_received`,
/// `pairing_success`, `pairing_failed`, `pairing_canceled`).
///
/// Payload shapes — must match plain-app's NearbyPairManager:
/// - `PAIRING_REQUEST_RECEIVED` → raw `PairingRequest` JSON
///   (the browser parses it as a `PairingRequest`)
/// - `PAIRING_SUCCESS` / `PAIRING_FAILED` / `PAIRING_CANCELED` → `DPairingResult`
///   JSON: `{ deviceId, deviceName, error }`
fn forward_pairing_event_to_ws(
    ws_event_tx: &tokio::sync::broadcast::Sender<
        crate::local::graphql::context::WsEvent,
    >,
    ev: &crate::local::pairing::PairingEvent,
) {
    use crate::local::graphql::context::{
        WsEvent, WS_PAIRING_CANCELLED, WS_PAIRING_FAILED,
        WS_PAIRING_REQUEST_RECEIVED, WS_PAIRING_STARTED, WS_PAIRING_SUCCESS,
    };
    use crate::local::pairing::PairingEventKind;

    let (event_type, payload) = match &ev.kind {
        PairingEventKind::IncomingRequest {
            request,
            sender_ip: _,
        } => {
            // plain-app sends the raw PairingRequest for `PAIRING_REQUEST_RECEIVED`.
            // Re-emit as raw JSON so the browser can parse it directly.
            match serde_json::to_string(request) {
                Ok(s) => (WS_PAIRING_REQUEST_RECEIVED, s),
                Err(_) => return,
            }
        }
        PairingEventKind::Started => {
            let result = DPairingResult {
                device_id: &ev.device_id,
                device_name: &ev.device_name,
                error: "",
            };
            match serde_json::to_string(&result) {
                Ok(s) => (WS_PAIRING_STARTED, s),
                Err(_) => return,
            }
        }
        PairingEventKind::Success => {
            let result = DPairingResult {
                device_id: &ev.device_id,
                device_name: &ev.device_name,
                error: "",
            };
            match serde_json::to_string(&result) {
                Ok(s) => (WS_PAIRING_SUCCESS, s),
                Err(_) => return,
            }
        }
        PairingEventKind::Failed { reason } => {
            let result = DPairingResult {
                device_id: &ev.device_id,
                device_name: &ev.device_name,
                error: reason,
            };
            match serde_json::to_string(&result) {
                Ok(s) => (WS_PAIRING_FAILED, s),
                Err(_) => return,
            }
        }
        PairingEventKind::Cancelled => {
            let result = DPairingResult {
                device_id: &ev.device_id,
                device_name: &ev.device_name,
                error: "",
            };
            match serde_json::to_string(&result) {
                Ok(s) => (WS_PAIRING_CANCELLED, s),
                Err(_) => return,
            }
        }
    };
    let _ = ws_event_tx.send(WsEvent {
        event_type,
        payload,
    });
}

#[cfg(test)]
mod tests {
    use super::{
        any_window_visible, keeps_process_alive, select_reopen_window_label,
        should_check_app_exit_after_destroy,
    };

    #[test]
    fn relaunch_can_recover_a_hidden_dynamic_application_window() {
        assert_eq!(
            select_reopen_window_label([
                ("screen-capture-overlay-7", Some(false)),
                ("media-preview-warm", Some(false)),
                ("window-chat", Some(false)),
            ]),
            Some("window-chat".to_string())
        );
    }

    #[test]
    fn hidden_only_windows_do_not_keep_app_alive() {
        assert!(!any_window_visible([Some(false), Some(false)].into_iter()));
    }

    #[test]
    fn one_visible_window_keeps_app_alive() {
        assert!(any_window_visible([Some(false), Some(true)].into_iter()));
    }

    #[test]
    fn empty_window_set_exits() {
        assert!(!any_window_visible(std::iter::empty()));
    }

    #[test]
    fn unknown_visibility_counts_as_visible() {
        assert!(any_window_visible([None].into_iter()));
    }

    #[test]
    fn capture_overlay_destruction_never_runs_the_application_exit_rule() {
        assert!(!should_check_app_exit_after_destroy(
            "screen-capture-overlay-7"
        ));
        assert!(should_check_app_exit_after_destroy(
            "screen-capture-overlay-07"
        ));
        assert!(should_check_app_exit_after_destroy("main"));
        assert!(should_check_app_exit_after_destroy("window-chat"));
        assert!(should_check_app_exit_after_destroy("media-preview-1"));
    }

    #[test]
    fn capture_bootstrap_does_not_keep_a_closed_application_alive() {
        assert!(!keeps_process_alive("screen-capture-overlay-7"));
        assert!(keeps_process_alive("screen-capture-overlay-07"));
        assert!(keeps_process_alive("main"));
        assert!(keeps_process_alive("window-chat"));
        assert!(keeps_process_alive("media-preview-1"));
    }
}
