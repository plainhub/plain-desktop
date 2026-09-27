//! Assemble the shared [`AppCtx`] for the plain-nas host — the mirror
//! of what the desktop's `local/server/mod.rs` builds at startup. The
//! nas crate's `cmd::run` calls [`nas_app_ctx`] with its paths/config
//! and stays thin; everything host-independent (identity from kv,
//! discovery managers kept idle, media service with the single fjall
//! handle) lives here.
//!
//! Idle pieces, by design:
//! * `PeerStatusManager::start()` is NOT called — it is the desktop's
//!   outbound status-connector side (reconnect loops dialing peers);
//!   the NAS learns peer liveness through its `ChatDiscovery`.
//! * `NearbyDiscoverManager` is constructed but never `start()`ed —
//!   same reason: the NAS advertises/browses through `ChatDiscovery`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::RwLock;

use tokio::sync::broadcast;

use crate::api::chat::ChatState;
use crate::api::context::{AppCtx, ShellHooks, WsEvent};
use crate::api::discover::{NearbyDiscoverManager, PeerStatusManager};
use crate::api::AppIdentity;
use crate::library::db::LibraryDb;
use crate::media::config::Config;
use crate::prefs::Prefs;

/// Inputs only the nas host knows: filesystem paths, the loaded config,
/// the prefs store, the assembled chat stack and the WS event channel.
pub struct NasCtxInputs {
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub prefs: Arc<Prefs>,
    pub config: Arc<Config>,
    pub chat: Arc<ChatState>,
    pub event_tx: broadcast::Sender<WsEvent>,
}

/// Build the nas host's identity from the same preference keys
/// `ChatState::nas_init` reads (`client_id` / `signature_key_pair` /
/// `device_name` with the hostname fallback).
pub fn nas_identity(prefs: &Prefs) -> AppIdentity {
    let keypair = crate::media::kv::SignatureKey::new(prefs)
        .ensure_keypair()
        .map(|kp| crate::base64_encode(&kp))
        .unwrap_or_default();
    let device_name = {
        let name = crate::media::kv::device_display_name(prefs);
        if name.is_empty() {
            let host = crate::utils::hostname::get();
            if host.is_empty() {
                "NAS".to_string()
            } else {
                host
            }
        } else {
            name
        }
    };
    AppIdentity {
        client_id: crate::media::kv::server_client_id(prefs),
        device_name,
        ed25519_keypair: keypair,
    }
}

/// Host-shell seam for the headless NAS: notifications land in the log
/// file; the app version is the one `nas::version::set` installed.
struct NasShellHooks;

impl ShellHooks for NasShellHooks {
    fn notify(&self, event: &str, payload: String) {
        log::info!("[nas-shell] {event}: {payload}");
    }

    fn app_version(&self) -> String {
        crate::nas::version::full_version()
    }
}

/// Open (or create) `library.db` under `data_dir` — the single place
/// the nas host opens it (shared by the AppCtx and the gql schema).
pub fn open_library_db(data_dir: &Path) -> anyhow::Result<LibraryDb> {
    Ok(LibraryDb::open(&data_dir.join("library.db"))?)
}

/// Assemble the nas [`AppCtx`]. Also returns the opened media service
/// pieces the caller already needs (the single fjall handle).
pub fn nas_app_ctx(inputs: NasCtxInputs) -> anyhow::Result<Arc<AppCtx>> {
    let NasCtxInputs {
        data_dir,
        cache_dir,
        prefs,
        config,
        chat,
        event_tx,
    } = inputs;

    // The single fjall handle for the whole process (media rows,
    // sessions, events, trash) — also installs the process-global kv
    // default the file-task/trash/event-log paths use.
    let media = Arc::new(crate::media::service::MediaService::init(
        &data_dir,
        &cache_dir,
    )?);

    let identity = Arc::new(nas_identity(&prefs));
    let db = Arc::new(chat.service.db.clone());
    let library = Arc::new(open_library_db(&data_dir)?);

    let peer_status = PeerStatusManager::new(db.clone(), identity.clone());
    let device_name = Arc::new(RwLock::new(identity.device_name.clone()));
    let mdns_hostname = Arc::new(RwLock::new(
        crate::prefs::identity::ensure_mdns_hostname(&prefs),
    ));
    let https_port: u16 = config
        .get_string("server.https_port")
        .parse()
        .unwrap_or(8443);
    let discover_manager = NearbyDiscoverManager::new(
        db.clone(),
        identity.clone(),
        device_name.clone(),
        mdns_hostname,
        chat.clone(),
        peer_status.clone(),
        https_port,
        crate::nas::version::full_version(),
    );
    let dlna_engine = Arc::new(crate::api::dlna::receiver_engine::DlnaEngine::new());

    let port = Arc::new(std::sync::atomic::AtomicU16::new(
        config
            .get_string("server.http_port")
            .parse()
            .unwrap_or(8080),
    ));
    let https_port_atomic = Arc::new(std::sync::atomic::AtomicU16::new(https_port));

    let token = crate::media::kv::UrlToken::new(&prefs).ensure()?;
    let log_dir = data_dir.join("logs");

    Ok(Arc::new(AppCtx {
        db,
        library,
        prefs,
        identity,
        peer_status,
        discover_manager,
        chat,
        dlna_engine,
        event_tx,
        token,
        port,
        https_port: https_port_atomic,
        data_dir,
        log_dir,
        device_name,
        shell: Arc::new(NasShellHooks),
        media,
    }))
}
