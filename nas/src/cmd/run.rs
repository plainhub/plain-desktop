//! `plain-nas run` - launches the HTTP + WebSocket service.

use crate::api::auth::AppState;
use crate::api::server::build_router;
use crate::config::Config;
use crate::consts::AppPaths;
use crate::ws_hub::global as global_hub;
use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::signal::unix::{SignalKind, signal};

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
    // Thumbnail engine budgets ([thumbnails] mem_budget_mb / lru_mb).
    crate::media::thumb_engine::init_from_config(&cfg);
    plain_rs::tls::ensure_self_signed_pem(
        &paths.tls_cert,
        &paths.tls_key,
        &["plainnas.local".to_string(), "localhost".to_string()],
    )
    .context("ensure self-signed cert")?;
    let cfg_arc = Arc::new(cfg);

    let db_path = crate::db::default_db_path(&paths.data_dir);
    let db = crate::db::open(&db_path).context("open fjall")?;
    let db = Arc::new(db.clone());

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

    // Background thumbnail prefetcher ([thumbnails] prefetch / prefetch_per_sec).
    crate::media::thumb_engine::prefetch::init_from_config(&cfg_arc, db.clone(), prefs.clone());

    let _ = crate::db::UrlToken::new(&prefs).ensure();

    // Chat stack (plain-app contract): SQLite chat.db + pairing manager
    // over the shared plain_rs::chat module.
    let mut chat = crate::chat::ChatState::init(&paths.data_dir, &prefs).context("init chat")?;
    chat.start_discovery(&prefs);
    let chat_discovery = chat.discovery.clone();
    let chat = Arc::new(chat);
    crate::chat::spawn_event_bridge(&chat);

    // Scan and mount all discovered filesystems into /mnt/usbX based on the
    // persisted FSUUID<->usbX slot map (Go calls EnsureMountedUSBVolumes at
    // startup), then keep reconciling on udev block events.
    {
        let p = prefs.clone();
        std::mem::drop(tokio::task::spawn_blocking(move || {
            if let Err(e) = crate::automount::ensure_mounted_usb_volumes(&p) {
                crate::log::error!("storage mount ensure failed: {e}");
            }
            crate::automount::run_automount_watcher();
        }));
    }

    let hub = global_hub();
    let cors_policy = crate::api::cors::CorsPolicy::from_config(&cfg_arc);
    let schema = crate::gql::build_schema(
        db.clone(),
        prefs.clone(),
        cfg_arc.clone(),
        paths.data_dir.clone(),
        chat.clone(),
    );

    let state = AppState {
        config: cfg_arc.clone(),
        db: db.clone(),
        prefs: prefs.clone(),
        ws_hub: Arc::new(hub.clone()),
        cors: cors_policy,
        schema,
        chat,
    };
    let heal_prefs = prefs.clone();
    let app = build_router(state);

    // Rebuild derived indexes that are missing or empty (e.g. wiped by a
    // schema migration) from the KV source of truth; background so serving
    // starts immediately.
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

    let http_port: u16 = cfg_arc
        .get_string("server.http_port")
        .parse()
        .unwrap_or(8080);
    let https_port: u16 = cfg_arc
        .get_string("server.https_port")
        .parse()
        .unwrap_or(8443);

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
                    crate::log::error!("tls config: {e}");
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
            crate::log::info!("HTTPS server starting on :{https_port}");
            if let Err(e) = axum_server::bind_rustls(addr, cfg)
                .serve(app.into_make_service_with_connect_info::<std::net::SocketAddr>())
                .await
            {
                crate::log::error!("https server error: {e}");
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
                    crate::log::error!("http bind failed: {e}");
                    return;
                }
            };
            crate::log::info!("HTTP server starting on :{http_port}");
            if let Err(e) = axum::serve(listener, app).await {
                crate::log::error!("http server error: {e}");
            }
        }));
    }

    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;
    tokio::select! {
        _ = sigterm.recv() => crate::log::info!("SIGTERM received, shutting down"),
        _ = sigint.recv()  => crate::log::info!("SIGINT received, shutting down"),
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
