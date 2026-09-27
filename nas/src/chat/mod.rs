//! NAS chat wiring over the shared `plain_rs::chat` stack.
//!
//! One place assembles everything the plain-app chat contract needs on
//! the NAS:
//! * `chat.db` — the plain-app-schema SQLite store (chats / channels /
//!   peers / nearby cache / app files) under the data dir.
//! * identity — the server's existing `client_id` + Ed25519 signature
//!   keypair (the same one `/init` publishes) + display name.
//! * `UreqTransport` — the LAN HTTPS peer transport (peers serve
//!   self-signed certs, so verification is off, mirroring plain-app's
//!   `createUnsafeHttpClient`).
//!
//! GraphQL resolvers and the `/nearby` + `/peer_graphql` handlers reach
//! everything through [`ChatState`].

use std::sync::Arc;
use std::time::Duration;

use plain_rs::chat::db::ChatDb;
use plain_rs::chat::enums::DeviceType;
use plain_rs::chat::pairing::PairingManager;
use plain_rs::chat::service::{ChatHooks, ChatIdentity, ChatService, no_link_previews};
use plain_rs::chat::transport::PeerTransport;

use crate::db::{SignatureKey, UrlToken};

pub mod discovery;

/// ureq-based [`PeerTransport`] — blocking HTTP moved onto the blocking
/// pool so the async runtime is never stalled. Accepts self-signed peer
/// certificates (the LAN pairing model has no CA).
#[derive(Clone)]
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl UreqTransport {
    pub fn new() -> Self {
        Self {
            agent: unsafe_agent(),
        }
    }
}

impl Default for UreqTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl PeerTransport for UreqTransport {
    fn post<'a>(
        &'a self,
        url: &'a str,
        client_id: &'a str,
        channel_id: Option<&'a str>,
        body: &'a [u8],
    ) -> impl std::future::Future<Output = Result<Vec<u8>, String>> + Send {
        let url = url.to_string();
        let client_id = client_id.to_string();
        let channel_id = channel_id.map(str::to_string);
        let body = body.to_vec();
        let agent = self.agent.clone();
        async move {
            tokio::task::spawn_blocking(move || {
                let mut req = agent
                    .post(&url)
                    .set("Content-Type", "application/octet-stream")
                    .set("c-id", &client_id);
                if let Some(cid) = channel_id.as_deref() {
                    req = req.set("c-cid", cid);
                }
                let resp = req.send_bytes(&body).map_err(|e| format!("{e}"))?;
                let status = resp.status();
                if !(200..300).contains(&status) {
                    return Err(format!("HTTP {status}"));
                }
                let mut bytes = Vec::new();
                use std::io::Read;
                resp.into_reader()
                    .take(4 * 1024 * 1024)
                    .read_to_end(&mut bytes)
                    .map_err(|e| format!("read response: {e}"))?;
                Ok(bytes)
            })
            .await
            .map_err(|e| format!("blocking task failed: {e}"))?
        }
    }
}

/// Agent with certificate verification disabled — mirrors plain-app's
/// `createUnsafeHttpClient`. Peers on the LAN present self-signed certs
/// with IP hostnames; authenticity comes from the protocol's own
/// ECDH + Ed25519 layer, not TLS.
fn unsafe_agent() -> ureq::Agent {
    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
    use rustls::{DigitallySignedStruct, Error, SignatureScheme};

    #[derive(Debug)]
    struct NoVerify;

    impl ServerCertVerifier for NoVerify {
        fn verify_server_cert(
            &self,
            _end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &ServerName<'_>,
            _ocsp_response: &[u8],
            _now: UnixTime,
        ) -> Result<ServerCertVerified, Error> {
            Ok(ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            vec![
                SignatureScheme::ED25519,
                SignatureScheme::ECDSA_NISTP256_SHA256,
                SignatureScheme::ECDSA_NISTP384_SHA384,
                SignatureScheme::RSA_PSS_SHA256,
                SignatureScheme::RSA_PSS_SHA384,
                SignatureScheme::RSA_PKCS1_SHA256,
                SignatureScheme::RSA_PKCS1_SHA384,
            ]
        }
    }

    // Explicit ring provider — feature unification across the dependency
    // tree can enable both ring and aws-lc-rs, defeating auto-detection.
    let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("rustls protocol versions")
    .dangerous()
    .with_custom_certificate_verifier(Arc::new(NoVerify))
    .with_no_client_auth();
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .tls_config(Arc::new(tls))
        .build()
}

/// Delivery-failure hook: kick an mDNS re-browse so a failed peer
/// delivery (usually a changed IP/port) refreshes the peer row for the
/// next attempt. The discovery handle is filled by
/// [`ChatState::start_discovery`] — before that the hook is a no-op.
#[derive(Default)]
struct NasChatHooks {
    discovery: std::sync::OnceLock<std::sync::Arc<discovery::ChatDiscovery>>,
}

impl ChatHooks for NasChatHooks {
    fn rebrowse_peers(&self) {
        if let Some(d) = self.discovery.get() {
            d.rebrowse();
        }
    }
}

/// The assembled NAS chat stack — what every chat surface talks to.
pub struct ChatState {
    pub service: ChatService<UreqTransport>,
    pub pairing: PairingManager<UreqTransport>,
    /// The minimal mutation-only schema served (encrypted) on
    /// `/peer_graphql`.
    pub peer_schema: crate::gql::peer_schema::PeerSchema,
    /// LAN discovery (mDNS advertise + browse). `None` until
    /// [`Self::start_discovery`] runs — tests keep it off (no sockets).
    pub discovery: Option<std::sync::Arc<discovery::ChatDiscovery>>,
    hooks: std::sync::Arc<NasChatHooks>,
}

impl ChatState {
    /// Open (or create) `chat.db` under `data_dir` and assemble the
    /// service + pairing manager from the NAS's existing identity
    /// primitives (`client_id`, signature keypair, URL token, display
    /// name).
    pub fn init(data_dir: &std::path::Path, prefs: &crate::prefs::Prefs) -> anyhow::Result<Self> {
        let db = ChatDb::open(&data_dir.join("chat.db"))?;
        let token = UrlToken::new(prefs).ensure()?;
        let keypair = SignatureKey::new(prefs).ensure_keypair()?;
        use base64::Engine;
        let display_name = {
            let name = crate::db::device_display_name(prefs);
            if name.is_empty() {
                let host = plain_rs::hostname::get();
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
            crate::db::server_client_id(prefs),
            display_name,
            base64::engine::general_purpose::STANDARD.encode(keypair),
        ));
        let hooks = Arc::new(NasChatHooks::default());
        let transport = Arc::new(UreqTransport::new());
        let service = ChatService::new(
            db.clone(),
            token,
            identity.clone(),
            DeviceType::Nas,
            data_dir.to_path_buf(),
            transport.clone(),
            hooks.clone(),
            no_link_previews(),
        );
        let pairing = PairingManager::new(db, identity, "NAS", transport);
        Ok(Self {
            service,
            pairing,
            peer_schema: crate::gql::peer_schema::build_schema(),
            discovery: None,
            hooks,
        })
    }

    /// Start LAN discovery (mDNS responder + resident browser). Runtime
    /// entry point — not called from tests.
    pub fn start_discovery(&mut self, prefs: &crate::prefs::Prefs) {
        let d = discovery::ChatDiscovery::start(
            self.service.db.clone(),
            self.service.identity.clone(),
            prefs,
        );
        let _ = self.hooks.discovery.set(d.clone());
        self.discovery = Some(d);
    }
}

/// Bridge the ChatService + PairingManager broadcast channels onto the
/// global event bus (`chat:event`, see `consts::EVENT_CHAT`) so
/// registered WS clients receive the phone-protocol msg_types verbatim.
/// Must run inside the tokio runtime (call from `cmd::run`).
pub fn spawn_event_bridge(chat: &ChatState) {
    use crate::consts::EVENT_CHAT;
    use crate::eventbus::EventBus;

    let mut rx = chat.service.event_tx.subscribe();
    tokio::spawn(async move {
        while let Ok(ev) = rx.recv().await {
            EventBus::global().publish(EVENT_CHAT, chat_event_to_bus(&ev));
        }
    });

    let mut prx = chat.pairing.subscribe();
    tokio::spawn(async move {
        while let Ok(ev) = prx.recv().await {
            EventBus::global().publish(EVENT_CHAT, pairing_event_to_bus(&ev));
        }
    });
}

/// Phone-protocol WS numbers for chat events (context.rs vocabulary of
/// the plain-app client) — the shared chat module already emits them.
pub fn chat_event_to_bus(ev: &plain_rs::chat::events::ChatEvent) -> serde_json::Value {
    serde_json::json!({
        "msgType": ev.event_type,
        "payload": ev.payload,
    })
}

/// PairingEvent → phone-protocol WS number (plain-app
/// PairingRequestReceived/Started/Success/Failed/Canceled events).
pub fn pairing_event_ws_type(kind: &plain_rs::chat::pairing::PairingEventKind) -> i32 {
    use plain_rs::chat::pairing::PairingEventKind as K;
    match kind {
        K::IncomingRequest { .. } => 22,
        K::Success => 23,
        K::Failed { .. } => 24,
        K::Cancelled => 25,
        K::Started => 26,
    }
}

pub fn pairing_event_to_bus(ev: &plain_rs::chat::pairing::PairingEvent) -> serde_json::Value {
    serde_json::json!({
        "msgType": pairing_event_ws_type(&ev.kind),
        "payload": serde_json::to_value(ev).unwrap_or(serde_json::Value::Null),
    })
}

/// Test seam: assemble a ChatState against a test data dir.
#[cfg(test)]
pub(crate) fn test_state(data_dir: &std::path::Path) -> std::sync::Arc<ChatState> {
    let prefs = crate::prefs::Prefs::load(&crate::prefs::default_path(data_dir)).unwrap();
    std::sync::Arc::new(ChatState::init(data_dir, &prefs).unwrap())
}

#[cfg(test)]
#[path = "../../tests/unit/chat/mod.rs"]
mod tests;
