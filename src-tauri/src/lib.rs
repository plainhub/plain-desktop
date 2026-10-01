mod capture;
mod commands;
mod prefs;
mod shell;
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
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(if cfg!(debug_assertions) {
                    log::LevelFilter::Debug
                } else {
                    log::LevelFilter::Info
                })
                .level_for("tungstenite", log::LevelFilter::Warn)
                .level_for("tantivy", log::LevelFilter::Warn)
                .level_for("fjall", log::LevelFilter::Warn)
                .level_for("lsm_tree", log::LevelFilter::Warn)
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
        .manage(commands::media_preview_pool::MediaPreviewState::default())
        .manage(commands::screen_capture::runtime::ScreenCaptureRuntime::default())
        .setup(|app| {
            // The unified preferences store — created before anything
            // reads a preference (shortcut registration below reads
            // `capture_shortcut`). One Arc per process, shared with the
            // local API server; `<app_data_dir>/prefs.json` (the file
            // tauri-plugin-store used to write, loaded as-is).
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            let prefs = Arc::new(
                plain_rs::prefs::Prefs::load(&plain_rs::prefs::default_path(&data_dir))
                    .expect("prefs.json load"),
            );
            let saved_roots = plain_rs::media::kv::media_source::get(&prefs);
            let mut root_candidates: Vec<std::path::PathBuf> = saved_roots
                .iter()
                .map(std::path::PathBuf::from)
                .collect();
            root_candidates.extend(
                [
                    app.path().home_dir(),
                    app.path().desktop_dir(),
                    app.path().document_dir(),
                    app.path().download_dir(),
                    app.path().audio_dir(),
                    app.path().picture_dir(),
                    app.path().video_dir(),
                ]
                .into_iter()
                .flatten()
                .filter(|path| path.is_dir()),
            );
            #[cfg(target_os = "macos")]
            if let Ok(home) = app.path().home_dir() {
                let cloud = home.join("Library/Mobile Documents/com~apple~CloudDocs");
                if cloud.is_dir() {
                    root_candidates.push(cloud);
                }
            }
            let mut roots: Vec<std::path::PathBuf> = root_candidates
                .into_iter()
                .filter(|path| !path.as_os_str().is_empty())
                .map(|path| path.canonicalize().unwrap_or(path))
                .collect();
            roots.sort_by(|left, right| {
                left.components()
                    .count()
                    .cmp(&right.components().count())
                    .then_with(|| left.cmp(right))
            });
            let mut unique_roots: Vec<std::path::PathBuf> = Vec::new();
            for root in roots {
                let separate_cloud_root = cfg!(target_os = "macos")
                    && root.ends_with(std::path::Path::new(
                        "Library/Mobile Documents/com~apple~CloudDocs",
                    ));
                if separate_cloud_root
                    || !unique_roots.iter().any(|existing| root.starts_with(existing))
                {
                    unique_roots.push(root);
                }
            }
            let roots: Vec<String> = unique_roots
                .into_iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect();
            let media_sources_changed = roots != saved_roots;
            if media_sources_changed {
                if let Err(error) = plain_rs::media::kv::media_source::set(&prefs, &roots) {
                    log::warn!("media source initialization failed: {error}");
                }
            }
            app.handle().manage(prefs.clone());

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

            // Windows and macOS use per-capture ephemeral overlays created at
            // their final geometry, so there is nothing to prewarm.
            #[cfg(not(any(target_os = "windows", target_os = "macos")))]
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

            #[cfg(all(debug_assertions, target_os = "macos"))]
            if commands::screen_capture::selftest::armed() {
                commands::screen_capture::selftest::schedule_trigger(app.handle().clone());
            }

            let log_dir = app
                .path()
                .app_log_dir()
                .unwrap_or_else(|_| data_dir.join("logs"));
            let db_path = data_dir.join("plain.db");
            let db = match plain_rs::db::Db::open(&db_path) {
                Ok(d) => Arc::new(d),
                Err(e) => panic!("local_db open failed: {e}"),
            };
            let identity = Arc::new(crate::prefs::ensure_identity(&prefs));
            let chat_state = Arc::new(plain_rs::api::chat::ChatState::new(
                &db,
                &identity,
                identity.device_name.clone(),
                crate::prefs::ensure_url_token(&prefs),
                data_dir.clone(),
            ));
            app.handle().manage(chat_state.clone());
            let (event_tx, _) = tokio::sync::broadcast::channel(1024);
            tauri::async_runtime::block_on(async {
                chat_state.spawn_event_bridges(event_tx.clone(), {
                    let handle = app.handle().clone();
                    move |event: &plain_rs::chat::pairing::PairingEvent| {
                        use tauri::Emitter;
                        let _ = handle.emit("pairing-event", event.clone());
                    }
                });
            });
            plain_rs::api::server::events::spawn_media_event_bridge(event_tx.clone());
            let ctx = plain_rs::api::context::AppCtx::assemble(
                data_dir.clone(),
                data_dir.join("cache"),
                log_dir,
                prefs.clone(),
                chat_state.clone(),
                event_tx.clone(),
                Arc::new(shell::DesktopShell(app.handle().clone())),
                0,
                0,
            )
            .expect("assemble local API context");
            let media_roots: Vec<std::path::PathBuf> = plain_rs::media::kv::media_source::get(&prefs)
                .into_iter()
                .map(std::path::PathBuf::from)
                .filter(|path| path.is_dir())
                .collect();
            if !media_roots.is_empty() {
                if let Ok(watcher) = plain_rs::media::watcher::start_watching(ctx.media.db.clone(), &media_roots) {
                    app.handle().manage(std::sync::Mutex::new(watcher));
                }
                let needs_initial_scan = media_sources_changed
                    || ctx.media.db.scan_prefix(b"media:uuid:").next().is_none();
                let media_db = ctx.media.db.clone();
                let data_dir_for_index = data_dir.clone();
                tauri::async_runtime::spawn(async move {
                    let db_for_index = media_db.clone();
                    let roots_for_index = media_roots.clone();
                    let _ = tauri::async_runtime::spawn_blocking(move || {
                        plain_rs::media::watcher::build_missing_indexes(
                            &data_dir_for_index,
                            &db_for_index,
                            &roots_for_index,
                        );
                    }).await;
                    if needs_initial_scan {
                        if let Err(error) = plain_rs::media::scan::start_walk_and_scan_paths(
                            media_db,
                            media_roots,
                            std::path::PathBuf::from("/"),
                        ).await {
                            log::error!("initial media scan failed: {error}");
                        }
                    }
                });
            }
            let peer_status = ctx.peer_status.clone();
            let discover_mgr = ctx.discover_manager.clone();
            let dlna_engine = ctx.dlna_engine.clone();
            chat_state.attach_discovery(discover_mgr.clone());
            let peer_resolver: plain_rs::api::http_proxy::PeerResolver = {
                let mgr = discover_mgr.clone();
                Arc::new(move |id: &str| mgr.peer_address(id))
            };
            let state = plain_rs::api::server::ServerState::new(
                Arc::new(plain_rs::httpserver::mainschemas::build_schema()),
                Arc::new(plain_rs::httpserver::peerschemas::build_schema()),
                ctx,
                plain_rs::api::server::ServerSettings {
                    auth: plain_rs::api::server::AuthPolicy::LocalToken,
                    cors: plain_rs::api::server::cors::CorsPolicy::permissive_default(),
                    serve_spa: false,
                },
            );
            app.handle().manage(tauri::async_runtime::block_on(async {
                plain_rs::api::http_proxy::HttpProxyState::start(peer_resolver)
            }));
            let local_server_state = tauri::async_runtime::block_on(async {
                plain_rs::api::server::runtime::ServerRuntime::start(state).await
            });
            app.handle().manage(dlna_engine.clone());
            // Start the DLNA renderer at startup when the toggle is on.
            if plain_rs::prefs::dlna::enabled(&prefs) {
                let engine = dlna_engine.clone();
                let port = local_server_state.port();
                tauri::async_runtime::spawn(async move {
                    engine.start(port).await;
                });
            }
            peer_status.set_event_tx(event_tx.clone());
            discover_mgr.set_event_tx(event_tx);
            discover_mgr.set_shell(std::sync::Arc::new(shell::DesktopShell(
                app.handle().clone(),
            )));
            discover_mgr.start();
            peer_status.set_discover_manager(discover_mgr.clone());
            // setup runs on the main thread, outside the tokio runtime —
            // start() reaches tokio::spawn via open_socket for connectable peers.
            tauri::async_runtime::block_on(async {
                peer_status.start();
            });
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
            commands::prefs::prefs_get_all,
            commands::prefs::prefs_set,
            commands::prefs::prefs_remove,
            commands::prefs::prefs_clear,
            commands::discover::login_peer,
            commands::discover::logout_peer,
            commands::discover::list_login_peers,
            commands::discover::update_peer_name,
            commands::discover::mdns_snapshot,
            commands::discover::mdns_activity,
            commands::discover::mdns_browse,
            commands::discover::mdns_start_browse,
            commands::discover::mdns_stop_browse,
            commands::discover::mdns_get_hostname,
            commands::discover::mdns_set_hostname,
            commands::discover::mdns_firewall_status,
            commands::discover::fix_mdns_firewall,
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
            commands::screen_capture::commands::screen_capture_request_permission,
            commands::screen_capture::commands::screen_capture_open_permission_settings,
            commands::screen_capture::commands::screen_capture_shortcut_status,
            commands::screen_capture::commands::screen_capture_set_shortcut,
            commands::screen_capture::commands::screen_capture_report_client_error,
            commands::screen_capture::commands::screen_capture_report_bootstrap_error,
            commands::screen_capture::commands::screen_capture_invalidate_target,
            commands::screen_capture::commands::screen_capture_fail,
            commands::screen_capture::commands::screen_capture_cancel,
            commands::screen_capture::commands::screen_capture_unavailable,
            commands::server::http_proxy_port,
            commands::server::local_server_port,
            commands::server::local_server_https_port,
            commands::server::local_server_token,
            commands::server::local_ipv4_strs,
            commands::server::set_http_port,
            commands::server::set_https_port,
            commands::server::restart_server,
            commands::pairing::pair_device,
            commands::pairing::respond_pair_device,
            commands::pairing::cancel_pair_device,
            commands::pairing::get_device_identity,
            commands::pairing::set_device_name,
            commands::dlna::dlna_state,
            commands::dlna::dlna_set_enabled,
            commands::dlna::dlna_accept_cast,
            commands::dlna::dlna_reject_cast,
            commands::dlna::dlna_senders,
            commands::dlna::dlna_remove_sender,
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
