//! Shared types, WebSocket event infrastructure, and resolver context.

use crate::assembly::AppIdentity;
use crate::services::chat::desktop::ChatState;
use crate::services::discovery::local::NearbyDiscoverManager;
use crate::services::discovery::local::PeerStatusManager;
use plain_rs::db::Db;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU16;
use tokio::sync::broadcast;

pub use super::events::*;

/// All server-level dependencies bundled for injection into async-graphql resolvers.
/// Passed per-request via `Request::data(Arc<AppCtx>)`.
pub struct AppCtx {
    pub uploads: Arc<plain_rs::files::uploads::Runtime>,
    pub image_updates: Arc<plain_rs::media::images::editor::Updates>,
    pub pomodoro: Arc<crate::services::pomodoro::Service>,
    pub feed_sync: Arc<crate::services::feeds::SyncService>,
    pub db: Arc<Db>,
    /// The shared system/user preference stores — one in-process writer
    /// for each file; resolvers read/write through them.
    pub prefs: Arc<plain_rs::prefs::Prefs>,
    pub identity: Arc<AppIdentity>,
    pub peer_status: PeerStatusManager,
    pub discover_manager: NearbyDiscoverManager,
    /// The assembled chat stack (service + pairing manager + key caches).
    pub chat: Arc<ChatState>,
    pub dlna_engine: Arc<crate::services::casting::receiver::receiver_engine::DlnaEngine>,
    pub event_tx: broadcast::Sender<WsEvent>,
    pub token: String,
    pub port: Arc<AtomicU16>,
    pub https_port: Arc<AtomicU16>,
    /// App data directory — used by developer resolvers to locate app data.
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
    pub media: Arc<plain_media::scan::service::MediaService>,
}

impl AppCtx {
    #[allow(clippy::too_many_arguments)]
    pub fn assemble(
        data_dir: PathBuf,
        cache_dir: PathBuf,
        log_dir: PathBuf,
        prefs: Arc<plain_rs::prefs::Prefs>,
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
        let media = Arc::new(plain_media::scan::service::MediaService::init(
            &data_dir, &cache_dir,
        )?);
        let peer_status = PeerStatusManager::new(db.clone(), identity.clone());
        let device_name = Arc::new(std::sync::RwLock::new(identity.device_name.clone()));
        let mdns_hostname = Arc::new(std::sync::RwLock::new(
            plain_rs::prefs::ensure_mdns_hostname(&prefs),
        ));
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
        let shared = crate::assembly::shared_services::SharedServices::new(
            db.clone(),
            prefs.clone(),
            event_tx.clone(),
            data_dir.clone(),
            Arc::new(plain_rs::files::uploads::Runtime::default()),
        );
        Ok(Arc::new(Self {
            uploads: shared.uploads,
            image_updates: shared.image_updates,
            pomodoro: shared.pomodoro,
            feed_sync: shared.feeds,
            db,
            prefs,
            identity,
            peer_status,
            discover_manager,
            chat,
            dlna_engine: Arc::new(
                crate::services::casting::receiver::receiver_engine::DlnaEngine::new(),
            ),
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
    fn capabilities(&self) -> Vec<crate::graphql::resolvers::app::types::Capability> {
        Vec::new()
    }
    fn disks(&self) -> anyhow::Result<Vec<crate::graphql::resolvers::app::capability_types::Disk>> {
        anyhow::bail!("disk manager unavailable")
    }
    fn app_update(
        &self,
    ) -> anyhow::Result<crate::graphql::resolvers::app::capability_types::AppUpdate> {
        Ok(
            crate::graphql::resolvers::app::capability_types::AppUpdate {
                current_version: self.app_version(),
                latest_version: None,
                has_update: false,
                url: None,
            },
        )
    }
    fn samba_settings(
        &self,
        _prefs: &plain_rs::prefs::Prefs,
    ) -> anyhow::Result<crate::graphql::resolvers::app::capability_types::SambaSettings> {
        anyhow::bail!("LAN share unavailable")
    }
    fn set_samba_settings(
        &self,
        _prefs: &plain_rs::prefs::Prefs,
        _input: crate::graphql::resolvers::app::capability_types::SambaSettingsInput,
    ) -> anyhow::Result<()> {
        anyhow::bail!("LAN share unavailable")
    }
    fn set_samba_user_password(
        &self,
        _prefs: &plain_rs::prefs::Prefs,
        _password: &str,
    ) -> anyhow::Result<()> {
        anyhow::bail!("LAN share unavailable")
    }
    fn dlna_renderers(
        &self,
        _cid: &str,
    ) -> anyhow::Result<Vec<crate::graphql::resolvers::app::capability_types::DlnaRenderer>> {
        anyhow::bail!("DLNA sender unavailable")
    }
    fn dlna_cast(
        &self,
        _renderer_udn: &str,
        _url: &str,
        _title: &str,
        _mime: &str,
        _media_type: crate::graphql::resolvers::media::types::MediaDataType,
        _prefs: &plain_rs::prefs::Prefs,
    ) -> anyhow::Result<()> {
        anyhow::bail!("DLNA sender unavailable")
    }
    fn set_hostname(&self, _name: &str) -> anyhow::Result<()> {
        anyhow::bail!("hostname change unavailable")
    }
    fn format_disk(
        &self,
        _prefs: &Arc<plain_rs::prefs::Prefs>,
        _path: &str,
        _cid: &str,
    ) -> anyhow::Result<()> {
        anyhow::bail!("disk manager unavailable")
    }
    fn relaunch_app(&self) -> bool {
        false
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
