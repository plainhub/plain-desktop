//! Chat wiring over the shared `crate::chat` stack — the single
//! assembly both hosts (desktop Tauri shell, plain-nas server) use.
//!
//! One place assembles everything the plain-app chat contract needs:
//! * `plain.db` — the plain-app-schema SQLite store (chats / channels /
//!   peers / nearby cache / app files) opened by the caller (it also
//!   carries bookmarks through the shared core).
//! * identity — desktop: the Tauri `AppIdentity` (`client_id` +
//!   Ed25519 keypair); NAS: the fjall-kv prefs identity
//!   (`server_client_id` + `SignatureKey` + display name).
//! * `ReqwestTransport` — the LAN HTTPS peer transport (peers serve
//!   self-signed certs, so verification is off, mirroring plain-app's
//!   `createUnsafeHttpClient`).
//! * link previews — the desktop OpenGraph scraper behind the shared
//!   `LinkPreviewFn` seam (NAS passes the no-op).
//!
//! Hosts pick their flavor through [`ChatOptions`] ([`ChatState::new`]
//! for the desktop defaults, [`ChatState::nas_init`] for the NAS).
//! GraphQL resolvers, `/nearby` and `/peer_graphql` handlers reach the
//! service and pairing manager through [`ChatState`].

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::chat::enums::DeviceType;
use crate::chat::pairing::{PairingEvent, PairingEventKind, PairingManager};
use crate::chat::service::{ChatHooks, ChatIdentity, ChatService, LinkPreviewFn};
use crate::chat::transport::PeerTransport;
use crate::db::Db;

use crate::api::AppIdentity;
use crate::api::context::{
    WS_PAIRING_CANCELLED, WS_PAIRING_FAILED, WS_PAIRING_REQUEST_RECEIVED, WS_PAIRING_STARTED,
    WS_PAIRING_SUCCESS, WsEvent,
};
use crate::discover::NearbyDiscoverManager;

/// reqwest-based [`PeerTransport`] — the outbound side of peer chat.
/// Accepts self-signed peer certificates (the LAN pairing model has no
/// CA); authenticity comes from the protocol's own ECDH + Ed25519 layer.
#[derive(Clone)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .danger_accept_invalid_certs(true)
                .danger_accept_invalid_hostnames(true)
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(10))
                .build()
                .expect("peer transport client"),
        }
    }
}

impl Default for ReqwestTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl PeerTransport for ReqwestTransport {
    fn post<'a>(
        &'a self,
        url: &'a str,
        client_id: &'a str,
        channel_id: Option<&'a str>,
        body: &'a [u8],
    ) -> impl std::future::Future<Output = Result<Vec<u8>, String>> + Send {
        let mut req = self
            .client
            .post(url)
            .header("c-id", client_id)
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(body.to_vec());
        if let Some(cid) = channel_id {
            req = req.header("c-cid", cid);
        }
        async move {
            let response = req.send().await.map_err(|e| format!("{e}"))?;
            let status = response.status();
            if !status.is_success() {
                return Err(format!("HTTP {status}"));
            }
            let bytes = response
                .bytes()
                .await
                .map_err(|e| format!("read response: {e}"))?;
            Ok(bytes.to_vec())
        }
    }
}

/// Delivery-failure hook: kick an mDNS re-browse so a failed peer
/// delivery (usually a changed IP/port) refreshes the peer row for the
/// next attempt. The discovery handle is filled by
/// [`ChatState::attach_discovery`] — before that the hook is a no-op.
#[derive(Default)]
struct DesktopChatHooks {
    discovery: std::sync::OnceLock<NearbyDiscoverManager>,
}

impl ChatHooks for DesktopChatHooks {
    fn rebrowse_peers(&self) {
        if let Some(d) = self.discovery.get() {
            d.browse();
        }
    }
}

/// The desktop OpenGraph scraper behind the shared link-preview seam.
fn desktop_link_previews() -> LinkPreviewFn {
    Arc::new(|db, data_dir, content| {
        Box::pin(async move {
            super::link_preview::ensure_link_previews(&db, &data_dir, &content).await
        })
    })
}

/// Whether a peer answers the `DISCOVER:` liveness ping on `/nearby`.
/// Deliberately NOT the shared transport (10s timeout) — discovery scans
/// want unreachable peers to fail fast.
pub async fn nearby_discovery_ping_succeeds(target_ip: &str, target_port: u16) -> bool {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    let client = CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .danger_accept_invalid_hostnames(true)
            .timeout(Duration::from_millis(2_500))
            .build()
            .expect("discovery ping client")
    });
    let url = crate::utils::build_url::build_url("https", target_ip, target_port, "/nearby");
    client
        .post(&url)
        .header("Content-Type", "application/json")
        .body("DISCOVER:")
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

/// The assembled chat stack — what every chat surface talks to. Both
/// hosts share this exact type; only the [`ChatOptions`] differ.
pub struct ChatState {
    pub service: ChatService<ReqwestTransport>,
    pub pairing: PairingManager<ReqwestTransport>,
    /// Shared with the service and pairing manager so a runtime rename
    /// (`updateDeviceName`) propagates everywhere at once.
    pub identity: Arc<ChatIdentity>,
    /// Concrete desktop hook handle — [`Self::attach_discovery`] fills
    /// its discovery slot. `None` unless built via [`Self::new`].
    desktop_hooks: Option<Arc<DesktopChatHooks>>,
    /// Concrete NAS hook handle — [`Self::start_discovery`] fills its
    /// discovery slot. `None` unless built via [`Self::nas_init`].
    #[cfg(feature = "system")]
    nas_hooks: Option<Arc<NasChatHooks>>,
    /// LAN discovery (mDNS advertise + browse). `None` until
    /// [`Self::start_discovery`] runs — tests keep it off (no sockets).
    #[cfg(feature = "system")]
    pub discovery: Option<Arc<crate::chat_discovery::ChatDiscovery>>,
}

/// Full-option assembly inputs for [`ChatState`] — every host difference
/// (device type, wire platform, link previews, hooks) is a field here.
pub struct ChatOptions {
    pub db: Db,
    pub identity: Arc<ChatIdentity>,
    pub token: String,
    pub data_dir: PathBuf,
    pub device_type: DeviceType,
    pub platform: &'static str,
    pub link_previews: LinkPreviewFn,
    pub hooks: Arc<dyn ChatHooks>,
}

impl ChatState {
    /// Assemble the stack from explicit options.
    pub fn with_options(opts: ChatOptions) -> Self {
        let transport = Arc::new(ReqwestTransport::new());
        let service = ChatService::new(
            opts.db.clone(),
            opts.token,
            opts.identity.clone(),
            opts.device_type,
            opts.data_dir,
            transport.clone(),
            opts.hooks,
            opts.link_previews,
        );
        let pairing = PairingManager::new(opts.db, opts.identity.clone(), opts.platform, transport);
        Self {
            service,
            pairing,
            identity: opts.identity,
            desktop_hooks: None,
            #[cfg(feature = "system")]
            nas_hooks: None,
            #[cfg(feature = "system")]
            discovery: None,
        }
    }

    /// Desktop assembly — `COMPUTER` device type, OpenGraph link
    /// previews, desktop discovery hooks.
    pub fn new(
        db: &Db,
        identity: &AppIdentity,
        device_name: String,
        token: String,
        data_dir: PathBuf,
    ) -> Self {
        let chat_identity = Arc::new(ChatIdentity::new(
            identity.client_id.clone(),
            device_name,
            identity.ed25519_keypair.clone(),
        ));
        let hooks = Arc::new(DesktopChatHooks::default());
        let mut state = Self::with_options(ChatOptions {
            db: db.clone(),
            identity: chat_identity,
            token,
            data_dir,
            device_type: DeviceType::Computer,
            platform: "COMPUTER",
            link_previews: desktop_link_previews(),
            hooks: hooks.clone(),
        });
        state.desktop_hooks = Some(hooks);
        state
    }

    /// Hand the discovery manager to the chat hooks (re-browse on failed
    /// delivery). Called once the manager exists — before that the hook
    /// is a no-op.
    pub fn attach_discovery(&self, discovery: NearbyDiscoverManager) {
        if let Some(hooks) = self.desktop_hooks.as_ref() {
            let _ = hooks.discovery.set(discovery);
        }
    }

    /// Bridge the ChatService + PairingManager broadcast channels onto the
    /// local-server WS bus (`WsEvent`) and the Tauri `pairing-event`, so
    /// desktop (Tauri) and browser-only clients see identical state.
    /// Must run inside the async runtime.
    pub fn spawn_event_bridges<F>(
        &self,
        ws_event_tx: tokio::sync::broadcast::Sender<WsEvent>,
        notify_shell: F,
    ) where
        F: Fn(&PairingEvent) + Send + Sync + 'static,
    {
        // Chat events are field-identical to WsEvent — forward verbatim.
        let mut rx = self.service.event_tx.subscribe();
        let ws_tx = ws_event_tx.clone();
        tokio::spawn(async move {
            while let Ok(ev) = rx.recv().await {
                let _ = ws_tx.send(WsEvent::broadcast(ev.event_type, ev.payload));
            }
        });

        let mut prx = self.pairing.subscribe();
        tokio::spawn(async move {
            while let Ok(ev) = prx.recv().await {
                notify_shell(&ev);
                forward_pairing_event_to_ws(&ws_event_tx, &ev);
            }
        });
    }
}

/// Delivery-failure hook for the NAS: kick an mDNS re-browse so a failed
/// peer delivery (usually a changed IP/port) refreshes the peer row for
/// the next attempt. The discovery handle is filled by
/// [`ChatState::start_discovery`] — before that the hook is a no-op.
#[cfg(feature = "system")]
#[derive(Default)]
pub struct NasChatHooks {
    discovery: std::sync::OnceLock<Arc<crate::chat_discovery::ChatDiscovery>>,
}

#[cfg(feature = "system")]
impl ChatHooks for NasChatHooks {
    fn rebrowse_peers(&self) {
        if let Some(d) = self.discovery.get() {
            d.rebrowse();
        }
    }
}

#[cfg(feature = "system")]
impl ChatState {
    /// NAS assembly — open (or create) `plain.db` under `data_dir` and
    /// build the stack from the NAS's existing identity primitives
    /// (`client_id`, signature keypair, URL token, display name).
    pub fn nas_init(
        data_dir: &std::path::Path,
        prefs: &crate::prefs::Prefs,
    ) -> anyhow::Result<Self> {
        let db = Db::open(&data_dir.join("plain.db"))?;
        let token = crate::media::kv::UrlToken::new(prefs).ensure()?;
        let keypair = crate::media::kv::SignatureKey::new(prefs).ensure_keypair()?;
        let display_name = {
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
        let identity = Arc::new(ChatIdentity::new(
            crate::media::kv::server_client_id(prefs),
            display_name,
            crate::utils::base64::base64_encode(&keypair),
        ));
        let hooks = Arc::new(NasChatHooks::default());
        let mut state = Self::with_options(ChatOptions {
            db,
            identity,
            token,
            data_dir: data_dir.to_path_buf(),
            device_type: DeviceType::Nas,
            platform: "NAS",
            link_previews: crate::chat::service::no_link_previews(),
            hooks: hooks.clone(),
        });
        state.nas_hooks = Some(hooks);
        Ok(state)
    }

    /// Start LAN discovery (mDNS responder + resident browser). Runtime
    /// entry point — not called from tests.
    pub fn start_discovery(
        &mut self,
        prefs: &crate::prefs::Prefs,
    ) -> Arc<crate::chat_discovery::ChatDiscovery> {
        let d = crate::chat_discovery::ChatDiscovery::start(
            self.service.db.clone(),
            self.service.identity.clone(),
            prefs,
        );
        if let Some(hooks) = self.nas_hooks.as_ref() {
            let _ = hooks.discovery.set(d.clone());
        }
        self.discovery = Some(d.clone());
        d
    }
}

/// Wire-format struct mirroring plain-app's `DPairingResult`
/// (`app/src/main/java/com/ismartcoding/plain/data/DNearbyPair.kt`).
/// Sent over the WebSocket for `PAIRING_SUCCESS` / `PAIRING_FAILED` /
/// `PAIRING_CANCELED` so the browser sees a single flat shape regardless
/// of whether the WebSocket is served by the desktop local server or
/// plain-app's Android HTTP server.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DPairingResult<'a> {
    device_id: &'a str,
    device_name: &'a str,
    error: &'a str,
}

/// Translate a `PairingEvent` into the WS wire pair (event-type number +
/// payload JSON string). Payload shapes — must match plain-app's
/// NearbyPairManager:
/// - `PAIRING_REQUEST_RECEIVED` → raw `PairingRequest` JSON
///   (the browser parses it as a `PairingRequest`)
/// - `PAIRING_SUCCESS` / `PAIRING_FAILED` / `PAIRING_CANCELED` /
///   `PAIRING_STARTED` → `DPairingResult` JSON:
///   `{ deviceId, deviceName, error }`
///
/// Shared by the desktop WS bridge and hosts that re-publish pairing
/// events on their own buses (plain-nas's eventbus).
pub fn pairing_event_ws_payload(ev: &PairingEvent) -> Option<(i32, String)> {
    let result = DPairingResult {
        device_id: &ev.device_id,
        device_name: &ev.device_name,
        error: "",
    };
    let (event_type, payload) = match &ev.kind {
        // plain-app sends the raw PairingRequest for `PAIRING_REQUEST_RECEIVED`.
        // Re-emit as raw JSON so the browser can parse it directly.
        PairingEventKind::IncomingRequest {
            request,
            sender_ip: _,
        } => (WS_PAIRING_REQUEST_RECEIVED, serde_json::to_string(request)),
        PairingEventKind::Started => (WS_PAIRING_STARTED, serde_json::to_string(&result)),
        PairingEventKind::Success => (WS_PAIRING_SUCCESS, serde_json::to_string(&result)),
        PairingEventKind::Failed { reason } => (
            WS_PAIRING_FAILED,
            serde_json::to_string(&DPairingResult {
                error: reason,
                ..result
            }),
        ),
        PairingEventKind::Cancelled => (WS_PAIRING_CANCELLED, serde_json::to_string(&result)),
    };
    Some((event_type, payload.ok()?))
}

/// Push a `PairingEvent` to the local GraphQL WebSocket as the
/// appropriate `WsEvent`, mirroring the event-type constants used by the
/// browser-side `app-socket.ts` (`pairing_request_received`,
/// `pairing_success`, `pairing_failed`, `pairing_canceled`).
fn forward_pairing_event_to_ws(
    ws_event_tx: &tokio::sync::broadcast::Sender<WsEvent>,
    ev: &PairingEvent,
) {
    let Some((event_type, payload)) = pairing_event_ws_payload(ev) else {
        return;
    };
    let _ = ws_event_tx.send(WsEvent::broadcast(event_type, payload));
}

#[cfg(test)]
#[path = "../tests/unit/api/chat.rs"]
mod tests;

#[cfg(all(test, feature = "system"))]
#[path = "../tests/unit/api/chat_nas.rs"]
mod nas_tests;
