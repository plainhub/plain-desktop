//! `plain-nas run` - launches the HTTP + WebSocket service: assemble
//! state and serve the shared plain-rs router (`server::build_router`)
//! over HTTP + HTTPS listeners.

use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::signal::unix::{SignalKind, signal};

use plain_rs::api::context::{AppCtx, ShellHooks};
use plain_rs::http_server::main_schemas::capability_types;
use plain_rs::http_server::main_schemas::types::Capability;
use plain_rs::http_server::ServerState;
use plain_rs::http_server::build_router;
use plain_rs::http_server::{AuthPolicy, ServerSettings, events::spawn_media_event_bridge};

use crate::config::Config;
use crate::consts::AppPaths;

struct NasShell;

impl ShellHooks for NasShell {
    fn notify(&self, event: &str, payload: String) {
        log::info!("[shell] {event}: {payload}");
    }

    fn app_version(&self) -> String {
        plain_rs::system::version::full_version()
    }

    fn capabilities(&self) -> Vec<Capability> {
        let mut caps = Vec::new();
        if !plain_rs::storage::samba::detect_systemd_service_name().is_empty() {
            caps.push(Capability::LanShare);
        }
        if command_present("lsblk") {
            caps.push(Capability::DiskManager);
        }
        if command_present("libreoffice") || command_present("soffice") {
            caps.push(Capability::DocPreview);
        }
        caps
    }

    fn disks(&self) -> Result<Vec<capability_types::StorageDisk>> {
        Ok(plain_rs::storage::storage_disks::list_disks()
            .into_iter()
            .map(|disk| capability_types::StorageDisk {
                id: disk.id.into(),
                name: disk.name,
                path: disk.path,
                size_bytes: plain_rs::http_server::main_schemas::types::Long(disk.size_bytes),
                removable: disk.removable,
                model: disk.model,
            })
            .collect())
    }

    fn app_update(&self) -> Result<capability_types::AppUpdate> {
        let update = plain_rs::system::app_update::app_update();
        Ok(capability_types::AppUpdate {
            current_version: update.current_version,
            latest_version: update.latest_version,
            has_update: update.has_update,
            url: update.url,
        })
    }

    fn samba_settings(
        &self,
        prefs: &plain_rs::prefs::Prefs,
    ) -> Result<capability_types::SambaSettings> {
        let settings = plain_rs::storage::samba::get_samba_settings(prefs);
        let service = plain_rs::storage::samba::get_service_status();
        let (service_name, service_active, service_enabled) = if service.name.is_empty() {
            (
                settings.service_name,
                settings.service_active,
                settings.service_enabled,
            )
        } else {
            (service.name, service.active, service.enabled)
        };
        Ok(capability_types::SambaSettings {
            enabled: settings.enabled,
            username: settings.username,
            has_password: settings.has_password,
            shares: settings
                .shares
                .into_iter()
                .map(|share| capability_types::SambaShare {
                    name: share.name,
                    share_path: share.share_path,
                    auth: match share.auth {
                        plain_rs::storage::samba::SambaShareAuth::Guest => {
                            capability_types::SambaShareAuth::GUEST
                        }
                        plain_rs::storage::samba::SambaShareAuth::Password => {
                            capability_types::SambaShareAuth::PASSWORD
                        }
                    },
                    read_only: share.read_only,
                })
                .collect(),
            service_name,
            service_active,
            service_enabled,
        })
    }

    fn set_samba_settings(
        &self,
        prefs: &plain_rs::prefs::Prefs,
        input: capability_types::SambaSettingsInput,
    ) -> Result<()> {
        let previous = plain_rs::storage::samba::get_samba_settings(prefs);
        let mut needs_password = false;
        let shares = input
            .shares
            .into_iter()
            .map(|share| {
                let auth = match share.auth {
                    capability_types::SambaShareAuth::GUEST => {
                        plain_rs::storage::samba::SambaShareAuth::Guest
                    }
                    capability_types::SambaShareAuth::PASSWORD => {
                        needs_password = true;
                        plain_rs::storage::samba::SambaShareAuth::Password
                    }
                };
                plain_rs::storage::samba::SambaShare {
                    name: share.name,
                    share_path: share.share_path,
                    auth,
                    read_only: share.read_only,
                }
            })
            .collect();
        let mut desired = previous.clone();
        desired.enabled = input.enabled;
        desired.shares = shares;
        anyhow::ensure!(
            !desired.enabled || !desired.shares.is_empty(),
            "no shares configured"
        );
        anyhow::ensure!(
            !needs_password || previous.has_password,
            "password required"
        );
        plain_rs::storage::samba::set_samba_settings(prefs, &desired)?;
        let applied = plain_rs::storage::samba::get_samba_settings(prefs);
        plain_rs::storage::samba::apply(prefs, &applied, "")?;
        Ok(())
    }

    fn set_samba_user_password(
        &self,
        prefs: &plain_rs::prefs::Prefs,
        password: &str,
    ) -> Result<()> {
        plain_rs::storage::samba::set_user_password(password).map_err(anyhow::Error::msg)?;
        let mut settings = plain_rs::storage::samba::get_samba_settings(prefs);
        settings.has_password = true;
        plain_rs::storage::samba::set_samba_settings(prefs, &settings)?;
        if settings.enabled {
            let _ = plain_rs::storage::samba::apply(prefs, &settings, "");
        }
        Ok(())
    }

    fn dlna_renderers(&self, cid: &str) -> Result<Vec<capability_types::DlnaRenderer>> {
        if !cid.is_empty() {
            plain_rs::dlna_sender::start_renderer_discovery(cid);
        }
        Ok(plain_rs::dlna_sender::cached_renderers()
            .into_iter()
            .map(|renderer| capability_types::DlnaRenderer {
                udn: renderer.udn,
                name: renderer.name,
                manufacturer: (!renderer.manufacturer.is_empty()).then_some(renderer.manufacturer),
                model_name: (!renderer.model_name.is_empty()).then_some(renderer.model_name),
                location: renderer.location,
            })
            .collect())
    }

    fn dlna_cast(
        &self,
        renderer_udn: &str,
        url: &str,
        title: &str,
        mime: &str,
        media_type: plain_rs::http_server::main_schemas::types::MediaDataType,
        prefs: &plain_rs::prefs::Prefs,
    ) -> Result<()> {
        let kind = match media_type {
            plain_rs::http_server::main_schemas::types::MediaDataType::AUDIO => {
                plain_rs::dlna_sender::MediaType::Audio
            }
            plain_rs::http_server::main_schemas::types::MediaDataType::VIDEO => {
                plain_rs::dlna_sender::MediaType::Video
            }
            plain_rs::http_server::main_schemas::types::MediaDataType::IMAGE => {
                plain_rs::dlna_sender::MediaType::Image
            }
            plain_rs::http_server::main_schemas::types::MediaDataType::DOC => {
                anyhow::bail!("dlna_cast_doc_unsupported")
            }
        };
        plain_rs::dlna_sender::cast(renderer_udn, url, title, mime, kind, prefs)
            .map_err(anyhow::Error::msg)
    }

    fn set_hostname(&self, name: &str) -> Result<()> {
        let status = std::process::Command::new("hostnamectl")
            .arg("set-hostname")
            .arg(name)
            .status()?;
        anyhow::ensure!(status.success(), "device_name_set_hostname_failed");
        let _ = std::process::Command::new("systemctl")
            .args(["try-restart", "avahi-daemon"])
            .status();
        Ok(())
    }

    fn format_disk(
        &self,
        prefs: &Arc<plain_rs::prefs::Prefs>,
        path: &str,
        cid: &str,
    ) -> Result<()> {
        let on_unmount = |mount: &str| {
            let _ = plain_rs::media::kv::EventLog::new(plain_rs::media::kv::get_default())
                .add("unmount", mount, cid);
        };
        let result =
            plain_rs::storage::format_disk::format_disk_single_partition(prefs, path, on_unmount);
        plain_rs::media::eventbus::Bus::new().publish(
            plain_rs::media::eventbus::EVENT_DISK_FORMAT_DONE,
            serde_json::json!({"path": path, "ok": result.is_ok(), "error": result.as_ref().err().map(ToString::to_string)}),
        );
        let _ = plain_rs::media::kv::EventLog::new(plain_rs::media::kv::get_default()).add(
            if result.is_ok() {
                "format_disk"
            } else {
                "format_disk_failed"
            },
            path,
            cid,
        );
        result
    }
}

fn command_present(name: &str) -> bool {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .any(|path| path.join(name).is_file())
}

pub async fn run(paths: &AppPaths) -> Result<()> {
    if !is_root() && std::env::var("PLAIN_NAS_ALLOW_NONROOT").is_err() {
        eprintln!(
            "This command requires root privileges. Please run with sudo (or set PLAIN_NAS_ALLOW_NONROOT=1 for development)."
        );
        std::process::exit(1);
    }
    std::fs::create_dir_all(&paths.data_dir).context("mkdir data dir")?;
    // Host bootstrap for the shared media stack: pin its data/cache dirs
    // before anything (scan exclusions, thumb cache) probes them.
    plain_rs::media::paths::set(paths.data_dir.clone(), paths.cache_dir.clone());

    let cfg = Config::load(&paths.config_path);
    // Extra media-scan exclusions (comma-separated absolute paths), merged
    // with the built-in system/build-dir defaults.
    let extra: Vec<String> = cfg
        .get_string("media_scan.excluded_dirs")
        .split(',')
        .map(str::trim)
        .filter(|s| s.starts_with('/'))
        .map(str::to_string)
        .collect();
    crate::media_scan::set_extra_excluded_roots(extra);
    plain_rs::tls::ensure_self_signed_pem(
        &paths.tls_cert,
        &paths.tls_key,
        &["plainnas.local".to_string(), "localhost".to_string()],
    )
    .context("ensure self-signed cert")?;
    let cfg_arc = Arc::new(cfg);

    // Preferences (`<data_dir>/prefs.json`) — settings, device identity
    // and small app state; the fjall store holds row data only.
    let prefs = Arc::new(
        crate::prefs::Prefs::load(&crate::prefs::default_path(&paths.data_dir))
            .context("load prefs")?,
    );
    crate::prefs::set_global(prefs.clone());

    // On-disk log (`<data_dir>/logs/latest.log`) — the surface behind the
    // developer UI's logs page. stderr keeps mirroring every line.
    crate::log::set_file(&crate::log::default_log_file(&paths.data_dir));

    // Scan and mount all discovered filesystems into /mnt/usbX based on the
    // persisted FSUUID<->usbX slot map (Go calls EnsureMountedUSBVolumes at
    // startup), then keep reconciling on udev block events.
    {
        let p = prefs.clone();
        std::mem::drop(tokio::task::spawn_blocking(move || {
            if let Err(e) = crate::automount::ensure_mounted_usb_volumes(&p) {
                log::error!("storage mount ensure failed: {e}");
            }
            crate::automount::run_automount_watcher();
        }));
    }

    // Chat stack (plain-app contract): SQLite plain.db + pairing manager
    // over the shared plain_rs::chat module.
    let mut chat =
        plain_rs::chat_service::ChatState::nas_init(&paths.data_dir, &prefs).context("init chat")?;
    chat.start_discovery(&prefs);
    let chat_discovery = chat.discovery.clone();
    let chat = Arc::new(chat);

    // The unified WS event channel: chat/pairing bridges (phone-protocol
    // numbers) + the media/nas push bridge (41-45).
    let (event_tx, _) = tokio::sync::broadcast::channel::<plain_rs::api::context::WsEvent>(1024);
    chat.spawn_event_bridges(event_tx.clone(), |_ev| {});
    spawn_media_event_bridge(event_tx.clone());

    // The shared resolver context — one assembly, one fjall handle for
    // the whole process (media rows, sessions, events, trash).
    let http_port: u16 = cfg_arc
        .get_string("server.http_port")
        .parse()
        .unwrap_or(8080);
    let https_port: u16 = cfg_arc
        .get_string("server.https_port")
        .parse()
        .unwrap_or(8443);
    let ctx = AppCtx::assemble(
        paths.data_dir.clone(),
        paths.cache_dir.clone(),
        paths.data_dir.join("logs"),
        prefs.clone(),
        chat.clone(),
        event_tx,
        Arc::new(NasShell),
        http_port,
        https_port,
    )
    .context("assemble app ctx")?;
    let db = ctx.media.db.clone();

    // Thumbnail engine budgets ([thumbnails] mem_budget_mb / lru_mb) and
    // the background prefetcher ([thumbnails] prefetch / prefetch_per_sec).
    plain_rs::media::thumb::init_from_config(&cfg_arc);
    plain_rs::media::thumb::prefetch::init_from_config(&cfg_arc, db.clone(), prefs.clone());

    let cors_policy = plain_rs::http_server::routes::cors::CorsPolicy::from_config(&cfg_arc);
    let schema = plain_rs::http_server::main_schemas::build_schema();
    let state = ServerState::new(
        Arc::new(schema),
        Arc::new(plain_rs::http_server::peer_schemas::build_schema()),
        ctx,
        ServerSettings {
            auth: AuthPolicy::Session {
                dev_token: cfg_arc.get_string("auth.dev_token"),
                device_id: cfg_arc.get_string("nas.id"),
            },
            cors: cors_policy,
            serve_spa: true,
        },
    );
    let app = build_router(state);

    // Rebuild derived indexes that are missing or empty (e.g. wiped by a
    // schema migration) from the KV source of truth; background so serving
    // starts immediately.
    let heal_prefs = prefs.clone();
    let heal_db = db.clone();
    let heal_data_dir = paths.data_dir.clone();
    std::mem::drop(tokio::task::spawn_blocking(move || {
        // The NAS policy for a from-scratch file search index: the mounted
        // /mnt/usb* volumes (same logic the Go watcher used).
        let roots: Vec<std::path::PathBuf> = crate::mounts::list_mounts(&heal_prefs)
            .iter()
            .filter_map(|m| m.mount_point.as_deref())
            .map(str::trim)
            .filter(|mp| is_plain_nas_usb_mount(mp))
            .map(std::path::PathBuf::from)
            .collect();
        crate::watcher::build_missing_indexes(&heal_data_dir, &heal_db, &roots);
    }));

    let mut handles = Vec::new();

    // HTTPS listener via axum-server (handles rustls + axum body bridging).
    if !cfg_arc.get_string("server.https_port").is_empty() {
        let app = app.clone();
        let cert = paths.tls_cert.clone();
        let key = paths.tls_key.clone();
        handles.push(tokio::spawn(async move {
            let cfg = match axum_server::tls_rustls::RustlsConfig::from_pem_file(cert, key).await {
                Ok(c) => c,
                Err(e) => {
                    log::error!("tls config: {e}");
                    return;
                }
            };
            // The HTTPS port is what LAN peers dial — advertise it on
            // mDNS once it is about to bind (phones then discover this
            // NAS for pairing).
            if let Some(d) = chat_discovery.as_ref() {
                d.set_https_port(https_port);
            }
            let addr = std::net::SocketAddr::from(([0, 0, 0, 0], https_port));
            log::info!("HTTPS server starting on :{https_port}");
            if let Err(e) = axum_server::bind_rustls(addr, cfg)
                .serve(app.into_make_service_with_connect_info::<std::net::SocketAddr>())
                .await
            {
                log::error!("https server error: {e}");
            }
        }));
    }

    // HTTP listener.
    if !cfg_arc.get_string("server.http_port").is_empty() {
        let app = app.clone();
        handles.push(tokio::spawn(async move {
            let listener = match tokio::net::TcpListener::bind(("0.0.0.0", http_port)).await {
                Ok(l) => l,
                Err(e) => {
                    log::error!("http bind failed: {e}");
                    return;
                }
            };
            log::info!("HTTP server starting on :{http_port}");
            if let Err(e) = axum::serve(listener, app).await {
                log::error!("http server error: {e}");
            }
        }));
    }

    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;
    tokio::select! {
        _ = sigterm.recv() => log::info!("SIGTERM received, shutting down"),
        _ = sigint.recv()  => log::info!("SIGINT received, shutting down"),
    }
    for h in handles {
        h.abort();
    }

    // Flush the database on a blocking thread with a timeout so we don't hang forever.
    let db_clone = db.clone();
    let _ = tokio::task::spawn_blocking(move || {
        let _ = db_clone.flush();
    });

    // Give flush a brief moment, then exit regardless.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    Ok(())
}

fn is_root() -> bool {
    nix::unistd::Uid::current().is_root()
}

/// `/mnt/usb<positive int>` with no extra path segments (the automount
/// slot layout) — the roots the file search index covers.
fn is_plain_nas_usb_mount(mp: &str) -> bool {
    let mp = mp.trim_end_matches('/');
    let Some(n) = mp.strip_prefix("/mnt/usb") else {
        return false;
    };
    !n.is_empty() && !n.contains('/') && n.parse::<u32>().is_ok() && n != "0"
}
