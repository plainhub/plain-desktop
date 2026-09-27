//! Shared types, WebSocket event infrastructure, and resolver context.

use crate::api::AppIdentity;
use crate::api::chat::ChatState;
use crate::api::db::ChatDb;
use crate::api::discover::{NearbyDiscoverManager, PeerStatusManager};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU16;
use tokio::sync::broadcast;

pub use crate::chat::events::{WS_MESSAGE_UPDATED, WS_PEER_STATUS_UPDATED};

pub const WS_BOOKMARK_UPDATED: i32 = 15;
pub const WS_DEVICE_NAME_UPDATED: i32 = 21;
/// Peer file download progress — payload is a JSON array of
/// `DownloadProgressItem` (id, messageId, downloaded, total, speed, status).
/// Mirrors plain-app's `EventType.DOWNLOAD_PROGRESS`. The web client maps
/// event type 16 to `download_progress` (see `app-socket.ts`).
pub const WS_DOWNLOAD_PROGRESS: i32 = 16;
/// Mirrors plain-app's `PairingRequestReceivedEvent` — fired when the local
/// pairing manager receives an incoming PAIR_REQUEST that the user must
/// accept or reject. Payload is a `PairingEvent` JSON object.
pub const WS_PAIRING_REQUEST_RECEIVED: i32 = 22;
/// Mirrors plain-app's `PairingSuccessEvent` — fired when a pairing
/// handshake completes successfully.
pub const WS_PAIRING_SUCCESS: i32 = 23;
/// Mirrors plain-app's `PairingFailedEvent` — fired when a pairing
/// handshake fails or is rejected by the remote device.
pub const WS_PAIRING_FAILED: i32 = 24;
/// Mirrors plain-app's `PairingCanceledEvent` — fired when an in-progress
/// pairing is cancelled by either side.
pub const WS_PAIRING_CANCELLED: i32 = 25;
pub const WS_PAIRING_STARTED: i32 = 26;
/// Emitted for each LAN device that replied to a discover broadcast.
/// Payload is a single `DiscoveredDevice` JSON object.
pub const WS_NEARBY_DEVICE_FOUND: i32 = 27;
pub const WS_NEARBY_DEVICE_UNREACHABLE: i32 = 46;
/// Mirrors plain-app's `StartNearbyDiscoveryEvent` — fired when the
/// `startDiscovery` mutation kicks off the background scan loop.
pub const WS_NEARBY_DISCOVERY_STARTED: i32 = 29;
/// Mirrors plain-app's `StopNearbyDiscoveryEvent` — fired when the
/// `stopDiscovery` mutation tears the background scan loop down.
pub const WS_NEARBY_DISCOVERY_STOPPED: i32 = 30;
/// Result of an async chunk merge started by `mergeChunksAsync`. Payload is a
/// JSON object `{fileId, ok, value?, mergedSize?, error?}`. plain-app's event
/// enum occupies 1..=37 (with gaps), so this contract appends at 38.
pub const WS_UPLOAD_MERGE_RESULT: i32 = 38;

#[derive(Clone, Debug)]
pub struct WsEvent {
    pub event_type: i32,
    pub payload: String,
    /// Delivery target: `Some(cid)` delivers only to the socket whose cid
    /// matches (per-client media/file-task/DLNA progress); `None`
    /// broadcasts to every connected socket.
    pub target_cid: Option<String>,
}

impl WsEvent {
    pub fn broadcast(event_type: i32, payload: String) -> Self {
        Self {
            event_type,
            payload,
            target_cid: None,
        }
    }

    pub fn targeted(event_type: i32, payload: String, cid: &str) -> Self {
        Self {
            event_type,
            payload,
            target_cid: Some(cid.to_string()),
        }
    }
}

/// All server-level dependencies bundled for injection into async-graphql resolvers.
/// Passed per-request via `Request::data(Arc<AppCtx>)`.
pub struct AppCtx {
    pub db: Arc<ChatDb>,
    /// User library (audio queue/playlists/history, tags, favorite
    /// folders) — plain-rs `library` core over local_library.db.
    pub library: Arc<crate::api::db::LibraryDb>,
    /// The unified preferences store (`<data_dir>/prefs.json`) — the
    /// single in-process writer; resolvers read/write through it.
    pub prefs: Arc<crate::prefs::Prefs>,
    pub identity: Arc<AppIdentity>,
    pub peer_status: PeerStatusManager,
    pub discover_manager: NearbyDiscoverManager,
    /// The assembled chat stack (service + pairing manager + key caches).
    pub chat: Arc<ChatState>,
    pub dlna_engine: Arc<crate::api::dlna::receiver_engine::DlnaEngine>,
    pub event_tx: broadcast::Sender<WsEvent>,
    pub token: String,
    pub port: Arc<AtomicU16>,
    pub https_port: Arc<AtomicU16>,
    /// App data directory — used by debug resolvers to read prefs.json.
    pub data_dir: std::path::PathBuf,
    /// App log directory — used by debug resolvers to read/clear plain.log.
    pub log_dir: std::path::PathBuf,
    /// Mutable device display name — updated by the updateDeviceName mutation.
    pub device_name: Arc<std::sync::RwLock<String>>,
    /// Host-shell seam (preferences, UI notifications, app metadata).
    pub shell: Arc<dyn ShellHooks>,
    /// The media stack (fjall store, scan engine, thumbnails) behind the
    /// `media` feature — plain-desktop enables it for local file browsing.
    #[cfg(feature = "media")]
    pub media: Arc<crate::media::service::MediaService>,
}

impl AppCtx {
    #[allow(clippy::too_many_arguments)]
    pub fn assemble(
        data_dir: PathBuf,
        cache_dir: PathBuf,
        log_dir: PathBuf,
        library_path: PathBuf,
        prefs: Arc<crate::prefs::Prefs>,
        chat: Arc<ChatState>,
        event_tx: broadcast::Sender<WsEvent>,
        shell: Arc<dyn ShellHooks>,
        port: u16,
        https_port: u16,
    ) -> anyhow::Result<Arc<Self>> {
        let identity = Arc::new(AppIdentity {
            client_id: chat.identity.client_id.clone(),
            device_name: chat.identity.device_name(),
            ed25519_keypair: chat.identity.ed25519_keypair.clone(),
        });
        let db = Arc::new(chat.service.db.clone());
        let library = Arc::new(crate::library::db::LibraryDb::open(&library_path)?);
        let media = Arc::new(crate::media::service::MediaService::init(
            &data_dir, &cache_dir,
        )?);
        let peer_status = PeerStatusManager::new(db.clone(), identity.clone());
        let device_name = Arc::new(std::sync::RwLock::new(identity.device_name.clone()));
        let mdns_hostname = Arc::new(std::sync::RwLock::new(crate::prefs::ensure_mdns_hostname(
            &prefs,
        )));
        let discover_manager = NearbyDiscoverManager::new(
            db.clone(),
            identity.clone(),
            device_name.clone(),
            mdns_hostname,
            chat.clone(),
            peer_status.clone(),
            https_port,
            shell.app_version(),
        );
        let token = chat.service.token.clone();
        Ok(Arc::new(Self {
            db,
            library,
            prefs,
            identity,
            peer_status,
            discover_manager,
            chat,
            dlna_engine: Arc::new(crate::api::dlna::receiver_engine::DlnaEngine::new()),
            event_tx,
            token,
            port: Arc::new(AtomicU16::new(port)),
            https_port: Arc::new(AtomicU16::new(https_port)),
            data_dir,
            log_dir,
            device_name,
            shell,
            media,
        }))
    }
}

/// Host-shell integration seam: UI-only hooks the local API stack needs
/// from its host. All persisted state goes through [`AppCtx::prefs`]
/// instead — the shell no longer proxies preferences.
pub trait ShellHooks: Send + Sync {
    /// Notify the UI shell (desktop: Tauri webview event, payload JSON).
    fn notify(&self, event: &str, payload: String);
    /// App version reported by deviceInfo (desktop: package_info).
    fn app_version(&self) -> String {
        String::new()
    }
    fn capabilities(&self) -> Vec<crate::api::schema::types::Capability> {
        Vec::new()
    }
    fn disks(&self) -> anyhow::Result<Vec<crate::api::schema::capability_types::StorageDisk>> {
        anyhow::bail!("disk manager unavailable")
    }
    fn app_update(&self) -> anyhow::Result<crate::api::schema::capability_types::AppUpdate> {
        Ok(crate::api::schema::capability_types::AppUpdate {
            current_version: self.app_version(),
            latest_version: None,
            has_update: false,
            url: None,
        })
    }
    fn samba_settings(
        &self,
        _prefs: &crate::prefs::Prefs,
    ) -> anyhow::Result<crate::api::schema::capability_types::SambaSettings> {
        anyhow::bail!("LAN share unavailable")
    }
    fn set_samba_settings(
        &self,
        _prefs: &crate::prefs::Prefs,
        _input: crate::api::schema::capability_types::SambaSettingsInput,
    ) -> anyhow::Result<()> {
        anyhow::bail!("LAN share unavailable")
    }
    fn set_samba_user_password(
        &self,
        _prefs: &crate::prefs::Prefs,
        _password: &str,
    ) -> anyhow::Result<()> {
        anyhow::bail!("LAN share unavailable")
    }
    fn dlna_renderers(
        &self,
        _cid: &str,
    ) -> anyhow::Result<Vec<crate::api::schema::capability_types::DlnaRenderer>> {
        anyhow::bail!("DLNA sender unavailable")
    }
    fn dlna_cast(
        &self,
        _renderer_udn: &str,
        _url: &str,
        _title: &str,
        _mime: &str,
        _media_type: crate::media::gql::types::MediaDataType,
        _prefs: &crate::prefs::Prefs,
    ) -> anyhow::Result<()> {
        anyhow::bail!("DLNA sender unavailable")
    }
    fn set_hostname(&self, _name: &str) -> anyhow::Result<()> {
        anyhow::bail!("hostname change unavailable")
    }
    fn format_disk(
        &self,
        _prefs: &Arc<crate::prefs::Prefs>,
        _path: &str,
        _cid: &str,
    ) -> anyhow::Result<()> {
        anyhow::bail!("disk manager unavailable")
    }
}

pub struct LogShell {
    pub version: String,
}

impl ShellHooks for LogShell {
    fn notify(&self, event: &str, payload: String) {
        log::info!("[shell] {event}: {payload}");
    }

    fn app_version(&self) -> String {
        self.version.clone()
    }
}
