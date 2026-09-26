//! Desktop chat wiring over the shared `plain_rs::chat` stack.
//!
//! One place assembles everything the plain-app chat contract needs on
//! the desktop:
//! * `chat.db` — the plain-app-schema SQLite store (chats / channels /
//!   peers / nearby cache / app files) opened by the caller (it also
//!   carries the desktop-local bookmark tables, see `local::db`).
//! * identity — the Tauri `AppIdentity` (`client_id` + Ed25519 keypair)
//!   wrapped in the shared runtime-renamable `ChatIdentity`.
//! * `ReqwestTransport` — the LAN HTTPS peer transport (peers serve
//!   self-signed certs, so verification is off, mirroring plain-app's
//!   `createUnsafeHttpClient`).
//! * link previews — the desktop OpenGraph scraper behind the shared
//!   `LinkPreviewFn` seam.
//!
//! GraphQL resolvers, `/nearby` and `/peer_graphql` handlers reach the
//! service and pairing manager through [`ChatState`].

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use plain_rs::chat::db::ChatDb;
use plain_rs::chat::enums::DeviceType;
use plain_rs::chat::pairing::{PairingEvent, PairingEventKind, PairingManager};
use plain_rs::chat::service::{ChatHooks, ChatIdentity, ChatService, LinkPreviewFn};
use plain_rs::chat::transport::PeerTransport;

use crate::commands::discover::NearbyDiscoverManager;
use crate::local::graphql::context::{
    WS_PAIRING_CANCELLED, WS_PAIRING_FAILED, WS_PAIRING_REQUEST_RECEIVED, WS_PAIRING_STARTED,
    WS_PAIRING_SUCCESS, WsEvent,
};
use crate::prefs::AppIdentity;

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
    let url = crate::utils::build_url("https", target_ip, target_port, "/nearby");
    client
        .post(&url)
        .header("Content-Type", "application/json")
        .body("DISCOVER:")
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

/// The assembled desktop chat stack — what every chat surface talks to.
pub struct ChatState {
    pub service: ChatService<ReqwestTransport>,
    pub pairing: PairingManager<ReqwestTransport>,
    /// Shared with the service and pairing manager so a runtime rename
    /// (`updateDeviceName`) propagates everywhere at once.
    pub identity: Arc<ChatIdentity>,
    hooks: Arc<DesktopChatHooks>,
}

impl ChatState {
    pub fn new(
        db: &ChatDb,
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
        let transport = Arc::new(ReqwestTransport::new());
        let hooks = Arc::new(DesktopChatHooks::default());
        let service = ChatService::new(
            db.clone(),
            token,
            chat_identity.clone(),
            DeviceType::Computer,
            data_dir,
            transport.clone(),
            hooks.clone(),
            desktop_link_previews(),
        );
        let pairing = PairingManager::new(db.clone(), chat_identity.clone(), "COMPUTER", transport);
        Self {
            service,
            pairing,
            identity: chat_identity,
            hooks,
        }
    }

    /// Hand the discovery manager to the chat hooks (re-browse on failed
    /// delivery). Called once the manager exists — before that the hook
    /// is a no-op.
    pub fn attach_discovery(&self, discovery: NearbyDiscoverManager) {
        let _ = self.hooks.discovery.set(discovery);
    }

    /// Bridge the ChatService + PairingManager broadcast channels onto the
    /// local-server WS bus (`WsEvent`) and the Tauri `pairing-event`, so
    /// desktop (Tauri) and browser-only clients see identical state.
    /// Must run inside the async runtime.
    pub fn spawn_event_bridges(
        &self,
        ws_event_tx: tokio::sync::broadcast::Sender<WsEvent>,
        app_handle: tauri::AppHandle,
    ) {
        // Chat events are field-identical to WsEvent — forward verbatim.
        let mut rx = self.service.event_tx.subscribe();
        let ws_tx = ws_event_tx.clone();
        tauri::async_runtime::spawn(async move {
            while let Ok(ev) = rx.recv().await {
                let _ = ws_tx.send(WsEvent {
                    event_type: ev.event_type,
                    payload: ev.payload,
                });
            }
        });

        let mut prx = self.pairing.subscribe();
        tauri::async_runtime::spawn(async move {
            use tauri::Emitter;
            while let Ok(ev) = prx.recv().await {
                let _ = app_handle.emit("pairing-event", ev.clone());
                forward_pairing_event_to_ws(&ws_event_tx, &ev);
            }
        });
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
    ws_event_tx: &tokio::sync::broadcast::Sender<WsEvent>,
    ev: &PairingEvent,
) {
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
