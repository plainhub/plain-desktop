use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        let windows = app.webview_windows();
        let candidate = crate::select_reopen_window_label(
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
    let builder = if let Some(plugin) =
        crate::commands::screen_capture::shortcut::ordinary_shortcut_plugin()
    {
        builder.plugin(plugin)
    } else {
        builder
    };
    let builder = crate::native_resource_protocol::with_native_resource_protocol(builder);
    let app = builder
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Trace)
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
        .manage(crate::commands::media_preview_pool::MediaPreviewState::default())
        .manage(crate::commands::screen_capture::runtime::ScreenCaptureRuntime::default())
        .manage(crate::commands::native::uploads::UploadWriters::default())
        .manage(crate::commands::native::archives::ArchiveReaders::default())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            let core = Arc::new(
                tauri::async_runtime::block_on(async {
                    plain_server::assembly::server::ContentServer::open(&data_dir)
                })
                .expect("open the shared core"),
            );
            app.handle().manage(core.clone());
            let prefs = core.shared_preferences()?;
            let saved_roots = plain_rs::prefs::media::source_directories(&prefs)?;
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
                if let Err(error) = plain_rs::prefs::media::set_source_directories(&prefs, &roots) {
                    log::warn!("media source initialization failed: {error}");
                }
            }
            app.handle().manage(prefs.clone());

            #[cfg(target_os = "macos")]
            crate::commands::macos_menu::setup(app)?;

            #[cfg(target_os = "linux")]
            match crate::commands::screen_capture::shortcut::current_linux_shortcut_backend() {
                crate::commands::screen_capture::shortcut::LinuxShortcutBackend::OrdinaryPlugin => {
                    if let Err(error) =
                        crate::commands::screen_capture::shortcut::register_ordinary_capture_shortcut(
                            app.handle(),
                        )
                    {
                        log::warn!("screen capture shortcut registration failed: {error}");
                    }
                }
                crate::commands::screen_capture::shortcut::LinuxShortcutBackend::WaylandPortalRequired => {
                    let handle = app.handle().clone();
                    tauri::async_runtime::spawn(async move {
                        match crate::commands::screen_capture::shortcut::register_wayland_portal_capture_shortcut(
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
                crate::commands::screen_capture::shortcut::register_ordinary_capture_shortcut(app.handle())
            {
                log::warn!("screen capture shortcut registration failed: {error}");
            }

            // Windows and macOS use per-capture ephemeral overlays created at
            // their final geometry, so there is nothing to prewarm.
            #[cfg(not(any(target_os = "windows", target_os = "macos")))]
            {
                let runtime = app
                    .handle()
                    .state::<crate::commands::screen_capture::runtime::ScreenCaptureRuntime>();
                let windows = crate::commands::screen_capture::window::TauriCaptureWindowPort::new(
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
            if crate::commands::screen_capture::selftest::armed() {
                crate::commands::screen_capture::selftest::schedule_trigger(app.handle().clone());
            }

            let log_dir = app
                .path()
                .app_log_dir()
                .unwrap_or_else(|_| data_dir.join("logs"));
            let _ = log_dir;
            // The peer proxy dials by client id; the core owns the lookup.
            let peer_resolver: plain_server::http::proxy::local::PeerResolver = {
                let core = core.clone();
                Arc::new(move |id: &str| {
                    core.native_peers()
                        .ok()
                        .and_then(|peers| peers.address(id).ok().flatten())
                })
            };
            app.handle().manage(tauri::async_runtime::block_on(async {
                plain_server::http::proxy::local::HttpProxyState::start(peer_resolver)
            }));

            let capabilities = crate::host::device::desktop_capabilities()
                .iter()
                .map(|value| (*value).to_owned())
                .collect();
            let host_bridge = std::sync::Arc::new(crate::host::runtime::Runtime::new(
                core.clone(),
                app.handle().clone(),
                capabilities,
            ));
            let host_task = tauri::async_runtime::block_on(async { host_bridge.start() });
            tauri::async_runtime::spawn(async move {
                match host_task.await {
                    Ok(Ok(())) => {},
                    Ok(Err(error)) => log::error!("host bridge stopped: {error}"),
                    Err(error) => log::error!("host bridge task failed: {error}"),
                }
            });

            if let Ok(catalog) = core.native_media_catalog() {
                if let Err(error) =
                    catalog.configure_scan_directories(&data_dir.join("cache"), None)
                {
                    log::warn!("media scan configuration failed: {error}");
                }
                if media_sources_changed {
                    if let Err(error) = tauri::async_runtime::block_on(async {
                        catalog.start_source_scan("/")
                    }) {
                        log::warn!("initial media scan did not start: {error}");
                    }
                }
                if let Err(error) = catalog.watch_source_directories() {
                    log::warn!("media watch did not start: {error}");
                }
            }

            if let Err(error) = tauri::async_runtime::block_on(async {
                core.start_configured_public().await
            }) {
                panic!("start the public listener failed: {error}");
            }

            // Announce on the LAN after the listener is up, so peers can
            // actually reach the address the announcement carries.
            tauri::async_runtime::spawn(async move {
                match core.native_discovery() {
                    Ok(discovery) => {
                        if let Err(error) = discovery.publish().await {
                            log::warn!("mDNS announcement failed: {error}");
                        }
                    }
                    Err(error) => log::warn!("discovery unavailable: {error}"),
                }
            });
            Ok(())
        })
        .on_page_load(|webview, payload| {
            let app = webview.app_handle().clone();
            let runtime = app.state::<crate::commands::screen_capture::runtime::ScreenCaptureRuntime>();
            let windows = crate::commands::screen_capture::window::TauriCaptureWindowPort::new(app.clone());
            let overlay_generation =
                crate::commands::screen_capture::runtime::is_overlay_window_label(webview.label())
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
                    if crate::commands::screen_capture::runtime::is_overlay_window_label(webview.label()) {
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
                    if crate::commands::screen_capture::runtime::is_overlay_window_label(webview.label()) {
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
                crate::commands::screen_capture::commands::on_window_close_requested(
                    window.app_handle(),
                    window.label(),
                );
            }
            // Remember the frame while the window is still alive so the
            // dock-icon reopen can put it back exactly where it was.
            #[cfg(target_os = "macos")]
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                crate::commands::window::remember_main_window_frame(
                    window.app_handle(),
                    window.label(),
                );
            }
            if let tauri::WindowEvent::Destroyed = event {
                crate::commands::screen_capture::commands::on_window_destroyed(
                    window.app_handle(),
                    window.label(),
                );
                #[cfg(target_os = "macos")]
                crate::commands::macos_dock::remove_window_device_name(window.label());
                // Any preview window dying (warm or visible) means we no
                // longer have a ready window. Rebuild so the next click is
                // fast. The user explicitly asked to let the close path
                // destroy the window — we don't intercept.
                crate::commands::media_preview_pool::on_window_destroyed(
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
            crate::commands::prefs::prefs_get_all,
            crate::commands::prefs::prefs_set,
            crate::commands::prefs::prefs_remove,
            crate::commands::prefs::prefs_clear,
            crate::commands::discover::login_peer,
            crate::commands::discover::logout_peer,
            crate::commands::discover::list_login_peers,
            crate::commands::discover::update_peer_name,
            crate::commands::discover::mdns_snapshot,
            crate::commands::discover::mdns_activity,
            crate::commands::discover::mdns_browse,
            crate::commands::discover::mdns_start_browse,
            crate::commands::discover::mdns_stop_browse,
            crate::commands::discover::mdns_get_hostname,
            crate::commands::discover::mdns_set_hostname,
            crate::commands::discover::mdns_firewall_status,
            crate::commands::discover::fix_mdns_firewall,
            crate::commands::notification::send_macos_notification,
            crate::commands::updater::check_for_updates,
            #[cfg(target_os = "macos")]
            crate::commands::macos_menu::set_menu_locale,
            crate::commands::window::open_window,
            crate::commands::window::open_about_window,
            crate::commands::window::set_window_device_name,
            crate::commands::media_preview_pool::media_preview_init,
            crate::commands::media_preview_pool::media_preview_activate,
            crate::commands::reveal::reveal_chat_file,
            crate::commands::reveal::save_chat_file_as,
            crate::commands::reveal::save_text_file_as,
            crate::commands::reveal::copy_chat_file_to_clipboard,
            crate::commands::screen_capture::commands::screen_capture_register_target,
            crate::commands::screen_capture::commands::screen_capture_unregister_target,
            crate::commands::screen_capture::commands::screen_capture_start,
            crate::commands::screen_capture::commands::screen_capture_ready,
            crate::commands::screen_capture::commands::screen_capture_pending_frame,
            crate::commands::screen_capture::commands::screen_capture_take_frame,
            crate::commands::screen_capture::commands::screen_capture_frame_presented,
            crate::commands::screen_capture::commands::screen_capture_overlay_work_area,
            crate::commands::screen_capture::commands::screen_capture_submit_result,
            crate::commands::screen_capture::commands::screen_capture_send_result,
            crate::commands::screen_capture::commands::screen_capture_take_result,
            crate::commands::screen_capture::commands::screen_capture_release_result,
            crate::commands::screen_capture::commands::screen_capture_ack_result,
            crate::commands::screen_capture::commands::screen_capture_save_result,
            crate::commands::screen_capture::commands::screen_capture_copy_result,
            crate::commands::screen_capture::commands::screen_capture_discard_result,
            crate::commands::screen_capture::commands::screen_capture_request_permission,
            crate::commands::screen_capture::commands::screen_capture_open_permission_settings,
            crate::commands::screen_capture::commands::screen_capture_shortcut_status,
            crate::commands::screen_capture::commands::screen_capture_set_shortcut,
            crate::commands::screen_capture::commands::screen_capture_report_client_error,
            crate::commands::screen_capture::commands::screen_capture_report_bootstrap_error,
            crate::commands::screen_capture::commands::screen_capture_invalidate_target,
            crate::commands::screen_capture::commands::screen_capture_fail,
            crate::commands::screen_capture::commands::screen_capture_cancel,
            crate::commands::screen_capture::commands::screen_capture_unavailable,
            crate::commands::server::http_proxy_port,
            crate::commands::server::local_server_port,
            crate::commands::server::local_server_https_port,
            crate::commands::server::local_server_token,
            crate::commands::server::local_ipv4_strs,
            crate::commands::server::set_http_port,
            crate::commands::server::set_https_port,
            crate::commands::server::restart_server,
            crate::commands::pairing::pair_device,
            crate::commands::pairing::respond_pair_device,
            crate::commands::pairing::cancel_pair_device,
            crate::commands::pairing::get_device_identity,
            crate::commands::pairing::set_device_name,
            crate::commands::dlna::dlna_state,
            crate::commands::dlna::dlna_set_enabled,
            crate::commands::dlna::dlna_accept_cast,
            crate::commands::dlna::dlna_reject_cast,
            crate::commands::dlna::dlna_senders,
            crate::commands::dlna::dlna_remove_sender,
            crate::commands::native::graphql::native_graphql,
            crate::commands::native::events::native_events,
            crate::commands::native::resources::native_resource_open,
            crate::commands::native::resources::native_resource_close,
            crate::commands::native::resources::native_resource_close_owner,
            crate::commands::native::uploads::native_upload_chunks,
            crate::commands::native::uploads::native_upload_chunk_open,
            crate::commands::native::uploads::native_upload_chunk_write,
            crate::commands::native::uploads::native_upload_chunk_finish,
            crate::commands::native::uploads::native_upload_chunk_abort,
            crate::commands::native::uploads::native_upload_chunks_abort_owner,
            crate::commands::native::archives::native_archive_open,
            crate::commands::native::archives::native_archive_read,
            crate::commands::native::archives::native_archive_close,
            crate::commands::native::archives::native_archive_close_owner,
            crate::commands::native::file_tasks::native_file_create_directory,
            crate::commands::native::file_tasks::native_file_create_file,
            crate::commands::native::file_tasks::native_file_write_text,
            crate::commands::native::file_tasks::native_file_copy,
            crate::commands::native::file_tasks::native_file_move,
            crate::commands::native::file_tasks::native_file_tasks_list,
            crate::commands::native::file_tasks::native_file_task_recover,
            crate::commands::native::file_tasks::native_file_task_remove,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");
    app.run(crate::handle_run_event);
}
