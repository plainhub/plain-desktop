//! Peer-pairing protocol — mirrors plain-app `PairingCore` + `PairingMessenger`.
//!
//! The pairing handshake rides the LAN HTTPS transport (`POST /nearby`,
//! mirroring plain-app `NearbyRoutes` / `NearbyHttpClient`) instead of UDP:
//!   Initiator → Target : `PAIR_REQUEST:{json}`  (HTTPS POST to Target's ip:port)
//!   Target   → Initiator: `PAIR_RESPONSE:{json}` (HTTPS POST back)
//!   Either   → Other    : `PAIR_CANCEL:{json}`   (abort)
//!
//! Security:
//!   - ECDH P-256 ephemeral key exchange; 32-byte raw shared secret = XChaCha20 key.
//!   - Ed25519 signatures on canonical string to prevent MITM.
//!   - Timestamp in payload (±5 min) prevents replay attacks.
//!
//! The peer's HTTPS server presents a self-signed certificate and uses a local
//! IP as hostname, so the outbound client accepts invalid certs (mirrors
//! plain-app's `createUnsafeHttpClient`).
//!
//! After a successful handshake the peer is written to the `peers` table and the
//! ChatDb's key-cache is considered stale (callers should re-query `get_peers`).

use super::db::{ChatDb, DPeer, now_iso};
use crate::local::enums::{DeviceType, PeerStatus};
use crate::prefs::AppIdentity;
use plain_rs::{EcdhSession, base64_decode, base64_encode, ed25519_sign, ed25519_verify};
use serde::Serialize;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub mod commands;
pub mod protocol;
mod utils;

pub use protocol::{PairingCancel, PairingRequest, PairingResponse};

use utils::{local_ipv4_strs, now_ms, prefer_sender_ip, timestamp_ok};

const PAIR_REQUEST_PREFIX: &str = "PAIR_REQUEST:";
const PAIR_RESPONSE_PREFIX: &str = "PAIR_RESPONSE:";
const PAIR_CANCEL_PREFIX: &str = "PAIR_CANCEL:";
const DISCOVER_MESSAGE: &str = "DISCOVER:";
const DISCOVER_REPLY_MESSAGE: &str = "DISCOVER_REPLY:";
const LOCAL_DEVICE_TYPE_WIRE: &str = "COMPUTER";
/// Pairing is interactive; a stale/unreachable peer must fail fast.
const REQUEST_TIMEOUT_MS: u64 = 5_000;
/// Mirrors plain-app `PairingInitiator.PAIR_RESPONSE_TIMEOUT_MS`.
const PAIR_RESPONSE_TIMEOUT_MS: u64 = 90_000;

// ── Internal session state ────────────────────────────────────────────────────

struct PairingSession {
    device_name: String,
    device_ip: String,
    device_port: u16,
    /// Ephemeral ECDH session.  Consumed when shared key is derived.
    ecdh: Option<EcdhSession>,
}

// ── Pairing manager ───────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct PairingManager {
    pub db: Arc<ChatDb>,
    pub identity: Arc<AppIdentity>,
    sessions: Arc<Mutex<HashMap<String, PairingSession>>>,
    /// Broadcast channel to notify the frontend of pairing events.
    event_tx: tokio::sync::broadcast::Sender<PairingEvent>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PairingEvent {
    pub kind: PairingEventKind,
    pub device_id: String,
    pub device_name: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PairingEventKind {
    /// Incoming PAIR_REQUEST that the user must accept or reject.
    #[serde(rename_all = "camelCase")]
    IncomingRequest {
        request: Box<PairingRequest>,
        sender_ip: String,
    },
    /// PAIR_REQUEST was delivered; waiting for the peer's response.
    Started,
    /// Pairing completed successfully.
    Success,
    /// Pairing failed or was rejected.
    #[serde(rename_all = "camelCase")]
    Failed { reason: String },
    /// Remote cancelled pairing.
    Cancelled,
}

impl PairingManager {
    pub fn new(db: Arc<ChatDb>, identity: Arc<AppIdentity>) -> Self {
        let (tx, _rx) = tokio::sync::broadcast::channel(32);
        PairingManager {
            db,
            identity,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            event_tx: tx,
        }
    }

    /// Subscribe a new listener to the pairing-event broadcast. Use this to
    /// forward `PairingEvent`s to Tauri, to the local GraphQL WebSocket, or
    /// to any other consumer. Multiple subscribers are supported.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<PairingEvent> {
        self.event_tx.subscribe()
    }

    /// `POST /nearby` entry — mirrors plain-app `NearbyRoutes`. Returns
    /// `false` when the body carries no known message-type prefix.
    pub fn handle_nearby_post(&self, body: &str, remote_ip: &str) -> bool {
        if body.starts_with(DISCOVER_MESSAGE) || body.starts_with(DISCOVER_REPLY_MESSAGE) {
            true
        } else if let Some(payload) = body.strip_prefix(PAIR_REQUEST_PREFIX) {
            match serde_json::from_str::<PairingRequest>(payload) {
                Ok(req) => self.on_pair_request(req, remote_ip),
                Err(e) => log::debug!("local_pairing: bad PAIR_REQUEST: {e}"),
            }
            true
        } else if let Some(payload) = body.strip_prefix(PAIR_RESPONSE_PREFIX) {
            match serde_json::from_str::<PairingResponse>(payload) {
                Ok(resp) => self.on_pair_response(resp, remote_ip),
                Err(e) => log::debug!("local_pairing: bad PAIR_RESPONSE: {e}"),
            }
            true
        } else if let Some(payload) = body.strip_prefix(PAIR_CANCEL_PREFIX) {
            match serde_json::from_str::<PairingCancel>(payload) {
                Ok(cancel) => self.on_pair_cancel(cancel),
                Err(e) => log::debug!("local_pairing: bad PAIR_CANCEL: {e}"),
            }
            true
        } else {
            false
        }
    }

    // ── Initiator side ────────────────────────────────────────────────────────

    /// Send a PAIR_REQUEST to the peer's `POST /nearby` endpoint. Mirrors
    /// plain-app `PairingInitiator.start`: on delivery a `Started` event is
    /// emitted and a response timeout is armed; on failure the session is
    /// dropped and a `Failed` event emitted.
    pub fn start_pairing(
        &self,
        device_id: &str,
        device_name: &str,
        device_ip: &str,
        device_port: u16,
        local_port: u16,
    ) {
        let identity = &self.identity;
        let ecdh = EcdhSession::generate();
        let ecdh_pub_b64 = base64_encode(&ecdh.public_key_bytes);
        let kp_bytes = base64_decode(&identity.ed25519_keypair);
        let vk_bytes = if kp_bytes.len() == 64 {
            kp_bytes[32..].to_vec()
        } else {
            vec![]
        };
        let sig_pub_b64 = base64_encode(&vk_bytes);

        let ts = now_ms();
        let mut req = PairingRequest {
            from_id: identity.client_id.clone(),
            from_name: identity.device_name.clone(),
            port: local_port,
            device_type: LOCAL_DEVICE_TYPE_WIRE.to_string(),
            ecdh_public_key: ecdh_pub_b64,
            signature_public_key: sig_pub_b64,
            timestamp: ts,
            ips: local_ipv4_strs(),
            signature: String::new(),
            aware_supported: false,
            from_ip: String::new(),
        };
        req.signature = ed25519_sign(&kp_bytes, req.signature_data().as_bytes());

        {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.insert(
                device_id.to_string(),
                PairingSession {
                    device_name: device_name.to_string(),
                    device_ip: device_ip.to_string(),
                    device_port,
                    ecdh: Some(ecdh),
                },
            );
        }

        let msg = format!(
            "{}{}",
            PAIR_REQUEST_PREFIX,
            serde_json::to_string(&req).unwrap_or_default()
        );
        let mgr = self.clone();
        let device_id = device_id.to_string();
        let device_name = device_name.to_string();
        let target_ip = device_ip.to_string();
        tauri::async_runtime::spawn(async move {
            if post_nearby(&msg, &target_ip, device_port).await {
                let _ = mgr.event_tx.send(PairingEvent {
                    kind: PairingEventKind::Started,
                    device_id: device_id.clone(),
                    device_name: device_name.clone(),
                });
                mgr.arm_response_timeout(device_id, device_name).await;
            } else {
                mgr.sessions.lock().unwrap().remove(&device_id);
                let _ = mgr.event_tx.send(PairingEvent {
                    kind: PairingEventKind::Failed {
                        reason: "Failed to send pairing request".to_string(),
                    },
                    device_id,
                    device_name,
                });
            }
        });
    }

    /// Fails the pairing when the peer never answers. The session is removed
    /// by `on_pair_response` (or cancel) on the happy paths, so its continued
    /// existence after the timeout means no response arrived. Mirrors
    /// plain-app `PairingInitiator.awaitPairResponse`.
    async fn arm_response_timeout(&self, device_id: String, device_name: String) {
        tokio::time::sleep(Duration::from_millis(PAIR_RESPONSE_TIMEOUT_MS)).await;
        if self.sessions.lock().unwrap().remove(&device_id).is_some() {
            log::error!("local_pairing: response timeout for {device_name}");
            let _ = self.event_tx.send(PairingEvent {
                kind: PairingEventKind::Failed {
                    reason: "Pairing timed out".to_string(),
                },
                device_id,
                device_name,
            });
        }
    }

    // ── Responder side ────────────────────────────────────────────────────────

    /// Incoming PAIR_REQUEST from a remote device.  Emits `IncomingRequest` event;
    /// the frontend calls `respond_to_pairing(request_json, accepted)` to reply.
    fn on_pair_request(&self, mut req: PairingRequest, sender_ip: &str) {
        log::info!(
            "local_pairing: PAIR_REQUEST from_id={} from_name={} sender_ip={} timestamp={}",
            req.from_id,
            req.from_name,
            sender_ip,
            req.timestamp
        );
        if !timestamp_ok(req.timestamp) {
            log::warn!(
                "local_pairing: PAIR_REQUEST timestamp out of range now_diff_ms={}",
                now_ms() - req.timestamp
            );
            return;
        }
        if !ed25519_verify(
            &req.signature_public_key,
            req.signature_data().as_bytes(),
            &req.signature,
        ) {
            log::warn!("local_pairing: PAIR_REQUEST signature invalid");
            return;
        }
        log::info!("local_pairing: PAIR_REQUEST signature OK, emitting IncomingRequest");
        // Stamp the sender IP so the frontend can pass it back to
        // `respondToPairing` and the responder knows where to POST
        // the PAIR_RESPONSE. Mirrors plain-app's `handlePairRequest`
        // which sets `request.fromIp = senderAddress`.
        req.from_ip = sender_ip.to_string();
        let _ = self.event_tx.send(PairingEvent {
            kind: PairingEventKind::IncomingRequest {
                request: Box::new(req.clone()),
                sender_ip: sender_ip.to_string(),
            },
            device_id: req.from_id.clone(),
            device_name: req.from_name.clone(),
        });
    }

    /// Called from frontend after user accepts/rejects a pairing request.
    pub fn respond_to_pairing(
        &self,
        request: PairingRequest,
        sender_ip: &str,
        accepted: bool,
        local_port: u16,
    ) {
        let identity = &self.identity;
        let kp_bytes = base64_decode(&identity.ed25519_keypair);
        let vk_bytes = if kp_bytes.len() == 64 {
            kp_bytes[32..].to_vec()
        } else {
            vec![]
        };
        let sig_pub_b64 = base64_encode(&vk_bytes);
        let ts = now_ms();
        let target_ip = if sender_ip.is_empty() {
            request.from_ip.clone()
        } else {
            sender_ip.to_string()
        };

        if accepted {
            let ecdh = EcdhSession::generate();
            let ecdh_pub_b64 = base64_encode(&ecdh.public_key_bytes);

            let mut resp = PairingResponse {
                from_id: identity.client_id.clone(),
                to_id: request.from_id.clone(),
                port: local_port,
                device_type: LOCAL_DEVICE_TYPE_WIRE.to_string(),
                ecdh_public_key: ecdh_pub_b64,
                signature_public_key: sig_pub_b64,
                accepted: true,
                timestamp: ts,
                ips: local_ipv4_strs(),
                signature: String::new(),
                aware_supported: false,
            };
            resp.signature = ed25519_sign(&kp_bytes, resp.signature_data().as_bytes());

            let req_pub_bytes = base64_decode(&request.ecdh_public_key);
            log::info!(
                "local_pairing: respond_to_pairing accepted=true req_pub_len={} sender_ip={}",
                req_pub_bytes.len(),
                target_ip
            );
            if let Some(shared) = ecdh.compute_shared_key(&req_pub_bytes) {
                let peer_ips = prefer_sender_ip(&request.ips, &target_ip);
                let token = self
                    .db
                    .get_peer_by_id(&request.from_id)
                    .map(|p| p.token)
                    .unwrap_or_default();
                let peer = DPeer {
                    id: request.from_id.clone(),
                    name: request.from_name.clone(),
                    ip: peer_ips.clone(),
                    key: base64_encode(&shared),
                    public_key: request.signature_public_key.clone(),
                    status: PeerStatus::Paired,
                    port: request.port,
                    device_type: DeviceType::from_str(&request.device_type)
                        .unwrap_or(DeviceType::Unknown),
                    token,
                    created_at: now_iso(),
                    updated_at: now_iso(),
                };
                log::info!(
                    "local_pairing: inserting peer id={} name={} ip={} port={} device_type={}",
                    peer.id,
                    peer.name,
                    peer.ip,
                    peer.port,
                    peer.device_type
                );
                self.db.upsert_peer(&peer);
                let msg = format!(
                    "{}{}",
                    PAIR_RESPONSE_PREFIX,
                    serde_json::to_string(&resp).unwrap_or_default()
                );
                let target_port = request.port;
                tauri::async_runtime::spawn(async move {
                    post_nearby(&msg, &target_ip, target_port).await;
                });
                let _ = self.event_tx.send(PairingEvent {
                    kind: PairingEventKind::Success,
                    device_id: request.from_id.clone(),
                    device_name: request.from_name.clone(),
                });
            } else {
                log::error!("local_pairing: ECDH shared key computation failed");
            }
        } else {
            let mut resp = PairingResponse {
                from_id: identity.client_id.clone(),
                to_id: request.from_id.clone(),
                port: local_port,
                device_type: LOCAL_DEVICE_TYPE_WIRE.to_string(),
                ecdh_public_key: String::new(),
                signature_public_key: sig_pub_b64,
                accepted: false,
                timestamp: ts,
                ips: vec![],
                signature: String::new(),
                aware_supported: false,
            };
            resp.signature = ed25519_sign(&kp_bytes, resp.signature_data().as_bytes());
            let msg = format!(
                "{}{}",
                PAIR_RESPONSE_PREFIX,
                serde_json::to_string(&resp).unwrap_or_default()
            );
            let target_port = request.port;
            tauri::async_runtime::spawn(async move {
                post_nearby(&msg, &target_ip, target_port).await;
            });
        }
    }

    // ── Initiator receives response ───────────────────────────────────────────

    fn on_pair_response(&self, resp: PairingResponse, sender_ip: &str) {
        if !timestamp_ok(resp.timestamp) {
            log::warn!("local_pairing: PAIR_RESPONSE timestamp out of range");
            return;
        }
        if !ed25519_verify(
            &resp.signature_public_key,
            resp.signature_data().as_bytes(),
            &resp.signature,
        ) {
            log::warn!("local_pairing: PAIR_RESPONSE signature invalid");
            return;
        }

        let session = {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.remove(&resp.from_id)
        };

        let Some(session) = session else {
            log::debug!(
                "local_pairing: no session for PAIR_RESPONSE from {}",
                resp.from_id
            );
            return;
        };

        if !resp.accepted {
            let _ = self.event_tx.send(PairingEvent {
                kind: PairingEventKind::Failed {
                    reason: "Pairing request was rejected".to_string(),
                },
                device_id: resp.from_id.clone(),
                device_name: session.device_name.clone(),
            });
            return;
        }

        let Some(ecdh) = session.ecdh else {
            log::error!("local_pairing: session ECDH already consumed");
            return;
        };
        let resp_pub_bytes = base64_decode(&resp.ecdh_public_key);
        let Some(shared) = ecdh.compute_shared_key(&resp_pub_bytes) else {
            let _ = self.event_tx.send(PairingEvent {
                kind: PairingEventKind::Failed {
                    reason: "ECDH key computation failed".to_string(),
                },
                device_id: resp.from_id.clone(),
                device_name: session.device_name.clone(),
            });
            return;
        };

        let peer_ips = prefer_sender_ip(&resp.ips, sender_ip);
        let token = self
            .db
            .get_peer_by_id(&resp.from_id)
            .map(|p| p.token)
            .unwrap_or_default();
        let peer = DPeer {
            id: resp.from_id.clone(),
            name: session.device_name.clone(),
            ip: peer_ips,
            key: base64_encode(&shared),
            public_key: resp.signature_public_key.clone(),
            status: PeerStatus::Paired,
            port: resp.port,
            device_type: DeviceType::from_str(&resp.device_type).unwrap_or(DeviceType::Unknown),
            token,
            created_at: now_iso(),
            updated_at: now_iso(),
        };
        self.db.upsert_peer(&peer);
        let _ = self.event_tx.send(PairingEvent {
            kind: PairingEventKind::Success,
            device_id: resp.from_id.clone(),
            device_name: session.device_name.clone(),
        });
    }

    fn on_pair_cancel(&self, cancel: PairingCancel) {
        {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.remove(&cancel.from_id);
        }
        let _ = self.event_tx.send(PairingEvent {
            kind: PairingEventKind::Cancelled,
            device_id: cancel.from_id.clone(),
            device_name: String::new(),
        });
    }

    /// Cancel an in-progress pairing session (initiated by us). Mirrors
    /// plain-app `PairingInitiator.cancel`: sends PAIR_CANCEL to the peer,
    /// emits `Cancelled`, and drops the session.
    pub fn cancel_pairing(&self, device_id: &str) {
        let session = {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.remove(device_id)
        };
        if let Some(s) = session {
            let cancel = PairingCancel {
                from_id: self.identity.client_id.clone(),
                to_id: device_id.to_string(),
            };
            let msg = format!(
                "{}{}",
                PAIR_CANCEL_PREFIX,
                serde_json::to_string(&cancel).unwrap_or_default()
            );
            let target_ip = s.device_ip.clone();
            let target_port = s.device_port;
            tauri::async_runtime::spawn(async move {
                post_nearby(&msg, &target_ip, target_port).await;
            });
            let _ = self.event_tx.send(PairingEvent {
                kind: PairingEventKind::Cancelled,
                device_id: device_id.to_string(),
                device_name: s.device_name.clone(),
            });
            log::debug!("Pairing cancelled for device: {device_id}");
        }
    }

    /// Whether a pairing session is currently in progress for `device_id`.
    /// Mirrors plain-app's `NearbyViewModel.itemStatus[deviceId] == PAIRING`.
    pub fn is_pairing(&self, device_id: &str) -> bool {
        self.sessions.lock().unwrap().contains_key(device_id)
    }
}

// ── LAN HTTPS transport ───────────────────────────────────────────────────────

pub(crate) async fn nearby_discovery_ping_succeeds(target_ip: &str, target_port: u16) -> bool {
    let url = crate::utils::build_url("https", target_ip, target_port, "/nearby");
    nearby_client()
        .post(&url)
        .timeout(Duration::from_millis(2_500))
        .header("Content-Type", "application/json")
        .body(DISCOVER_MESSAGE)
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

/// Unsafe (self-signed-cert-trusting) HTTPS client for peer `POST /nearby`
/// calls — mirrors plain-app `NearbyHttpClient`.
fn nearby_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_millis(REQUEST_TIMEOUT_MS))
            .build()
            .expect("nearby https client")
    })
}

/// POSTs `body` to `https://[target_ip]:[target_port]/nearby`. Returns true
/// when the peer answered with a 2xx status.
async fn post_nearby(body: &str, target_ip: &str, target_port: u16) -> bool {
    let url = crate::utils::build_url("https", target_ip, target_port, "/nearby");
    let result = nearby_client()
        .post(&url)
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .await;
    match result {
        Ok(resp) => {
            let ok = resp.status().is_success();
            if !ok {
                log::error!("NearbyHttpClient: rejected {}", resp.status());
            }
            ok
        }
        Err(e) => {
            log::error!("NearbyHttpClient: failed {e}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearby_liveness_messages_are_accepted() {
        let path = std::env::temp_dir().join(format!(
            "plainapp-pairing-discover-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        let db = Arc::new(ChatDb::open(&path).unwrap());
        let identity = Arc::new(AppIdentity {
            client_id: "self".into(),
            device_name: "Desktop".into(),
            ed25519_keypair: String::new(),
        });
        let manager = PairingManager::new(db, identity);
        assert!(manager.handle_nearby_post("DISCOVER:", "127.0.0.1"));
        assert!(manager.handle_nearby_post("DISCOVER_REPLY:{}", "127.0.0.1"));
        assert!(!manager.handle_nearby_post("UNKNOWN:", "127.0.0.1"));
        drop(manager);
        let _ = std::fs::remove_file(path);
    }
}
