use super::nearby_discover_manager::NearbyDiscoverManager;
use crate::api::AppIdentity;
use crate::api::context::{WS_PEER_STATUS_UPDATED, WsEvent};
use crate::api::db::{DPeer, Db};
use crate::{base64_decode, chacha20_encrypt, ed25519_sign};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::{
    Arc, Mutex, RwLock,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;

const INITIAL_RECONNECT_DELAY_MS: u64 = 1_000;
const MAX_RECONNECT_DELAY_MS: u64 = 60_000;
/// Window a reconnect waits for the directed mDNS browse reply.
const DISCOVER_REPLY_WAIT_MS: u64 = 2_000;

#[derive(Default)]
struct PeerState {
    task: Option<tokio::task::JoinHandle<()>>,
    task_id: u64,
    reconnect_attempts: u32,
    pending_reconnect: bool,
    online: bool,
}

struct Inner {
    db: Arc<Db>,
    identity: Arc<AppIdentity>,
    states: Mutex<HashMap<String, PeerState>>,
    started: AtomicBool,
    event_tx: RwLock<Option<broadcast::Sender<WsEvent>>>,
    discover_manager: RwLock<Option<NearbyDiscoverManager>>,
}

#[derive(Clone)]
pub struct PeerStatusManager {
    inner: Arc<Inner>,
}

impl PeerStatusManager {
    pub fn new(db: Arc<Db>, identity: Arc<AppIdentity>) -> Self {
        Self {
            inner: Arc::new(Inner {
                db,
                identity,
                states: Mutex::new(HashMap::new()),
                started: AtomicBool::new(false),
                event_tx: RwLock::new(None),
                discover_manager: RwLock::new(None),
            }),
        }
    }

    pub fn set_event_tx(&self, event_tx: broadcast::Sender<WsEvent>) {
        *self.inner.event_tx.write().unwrap() = Some(event_tx);
    }

    pub fn set_discover_manager(&self, discover_manager: NearbyDiscoverManager) {
        *self.inner.discover_manager.write().unwrap() = Some(discover_manager);
    }

    pub fn start(&self) {
        if self.inner.started.swap(true, Ordering::SeqCst) {
            return;
        }
        self.reconnect_all();
    }

    #[allow(dead_code)]
    pub fn stop(&self) {
        self.inner.started.store(false, Ordering::SeqCst);
        let peer_ids: Vec<String> = {
            let mut states = self.inner.states.lock().unwrap();
            let ids = states.keys().cloned().collect::<Vec<_>>();
            for state in states.values_mut() {
                state.pending_reconnect = false;
                state.reconnect_attempts = 0;
                state.task_id = state.task_id.wrapping_add(1);
                if let Some(handle) = state.task.take() {
                    handle.abort();
                }
            }
            ids
        };
        for peer_id in peer_ids {
            self.set_online(&peer_id, false);
        }
    }

    pub fn disconnected(&self, peer_id: &str) {
        self.set_online(peer_id, false);
    }

    pub fn is_online(&self, peer_id: &str) -> bool {
        self.inner
            .states
            .lock()
            .unwrap()
            .get(peer_id)
            .map(|state| state.online)
            .unwrap_or(false)
    }

    #[allow(dead_code)]
    pub fn reconnect_now(&self, _reason: &str) {
        self.reconnect_all();
    }

    fn reconnect_all(&self) {
        if !self.inner.started.load(Ordering::SeqCst) {
            return;
        }
        for peer in self.load_connectable_peers() {
            self.force_reconnect_peer(peer, "reconnect_all");
        }
    }

    fn reconnect_peer(&self, peer_id: &str, _reason: &str) {
        if !self.inner.started.load(Ordering::SeqCst) {
            return;
        }
        {
            let mut states = self.inner.states.lock().unwrap();
            let state = states.entry(peer_id.to_string()).or_default();
            state.pending_reconnect = false;
            if state.task.is_some() {
                return;
            }
        }

        let Some(peer) = self.inner.db.get_peer_by_id(peer_id) else {
            return;
        };
        if !self.should_connect(&peer) {
            return;
        }
        let key = base64_decode(&peer.key);
        if key.len() != 32 {
            return;
        }

        // Record updated_at before the directed browse so we can detect
        // whether the reply actually arrived. Without this check, a stale IP
        // in the DB would be reused forever — the socket would fail against
        // the dead address and schedule another reconnect, never giving the
        // discover reply a chance to refresh the IP (mirrors plain-app's
        // PeerStatusManager.reconnectPeer).
        let updated_at_before = peer.updated_at.clone();
        if let Some(discover_manager) = self.inner.discover_manager.read().unwrap().clone() {
            discover_manager.browse();
            std::thread::sleep(Duration::from_millis(DISCOVER_REPLY_WAIT_MS));
        }

        let refreshed_peer = self.inner.db.get_peer_by_id(&peer.id).unwrap_or(peer);
        if refreshed_peer.updated_at == updated_at_before {
            log::debug!(
                "peer status: no discover reply peer={} — rescheduling",
                refreshed_peer.id
            );
            self.schedule_reconnect(refreshed_peer.id.clone());
            return;
        }
        if refreshed_peer.ip.is_empty() || refreshed_peer.port == 0 {
            self.schedule_reconnect(refreshed_peer.id.clone());
            return;
        }
        self.open_socket(refreshed_peer, key);
    }

    fn force_reconnect_peer(&self, peer: DPeer, _reason: &str) {
        if !self.inner.started.load(Ordering::SeqCst) {
            return;
        }
        {
            let mut states = self.inner.states.lock().unwrap();
            let state = states.entry(peer.id.clone()).or_default();
            state.pending_reconnect = false;
            if state.task.is_some() {
                return;
            }
        }
        let key = base64_decode(&peer.key);
        if key.len() != 32 {
            return;
        }
        self.open_socket(peer, key);
    }

    fn open_socket(&self, peer: DPeer, key: Vec<u8>) {
        let peer_id = peer.id.clone();
        let local_client_id = self.inner.identity.client_id.clone();
        let timestamp = now_ms().to_string();
        let signature = ed25519_sign(
            &base64_decode(&self.inner.identity.ed25519_keypair),
            format!("{timestamp}{local_client_id}").as_bytes(),
        );
        if signature.is_empty() {
            self.schedule_reconnect(peer_id);
            return;
        }
        let Some(payload) = chacha20_encrypt(
            &key,
            format!("{signature}|{timestamp}|{local_client_id}").as_bytes(),
        ) else {
            self.schedule_reconnect(peer_id);
            return;
        };
        let url = crate::utils::build_url::build_url(
            "wss",
            peer.best_ip(),
            peer.port,
            &format!("/status?cid={local_client_id}"),
        );

        let task_id = {
            let mut states = self.inner.states.lock().unwrap();
            let state = states.entry(peer_id.clone()).or_default();
            if state.task.is_some() {
                return;
            }
            state.task_id = state.task_id.wrapping_add(1);
            state.task_id
        };

        let manager = self.clone();
        let peer_id_for_task = peer_id.clone();
        let handle = tokio::spawn(async move {
            let tls = match native_tls::TlsConnector::builder()
                .danger_accept_invalid_certs(true)
                .danger_accept_invalid_hostnames(true)
                .build()
            {
                Ok(tls) => tls,
                Err(err) => {
                    log::error!(
                        "peer status: tls init failed peer={} err={err}",
                        peer_id_for_task
                    );
                    manager.connection_closed(&peer_id_for_task, task_id);
                    return;
                }
            };
            let connector = tokio_tungstenite::Connector::NativeTls(tls);
            let Ok((mut ws, _)) = tokio_tungstenite::connect_async_tls_with_config(
                url.as_str(),
                None,
                false,
                Some(connector),
            )
            .await
            else {
                manager.connection_closed(&peer_id_for_task, task_id);
                return;
            };

            if ws.send(Message::Binary(payload)).await.is_err() {
                manager.connection_closed(&peer_id_for_task, task_id);
                return;
            }

            while let Some(message) = ws.next().await {
                match message {
                    Ok(Message::Text(text)) if text == "ok" => {
                        manager.connection_authenticated(&peer_id_for_task, task_id);
                    }
                    Ok(Message::Binary(bytes)) if &bytes[..] == b"ok" => {
                        manager.connection_authenticated(&peer_id_for_task, task_id);
                    }
                    Ok(Message::Close(_)) => break,
                    Err(err) => {
                        log::debug!(
                            "peer status: socket failed peer={} err={err}",
                            peer_id_for_task
                        );
                        break;
                    }
                    _ => {}
                }
            }
            manager.connection_closed(&peer_id_for_task, task_id);
        });

        let mut states = self.inner.states.lock().unwrap();
        let state = states.entry(peer_id.clone()).or_default();
        if state.task_id == task_id {
            state.task = Some(handle);
        } else {
            handle.abort();
        }
    }

    fn connection_authenticated(&self, peer_id: &str, task_id: u64) {
        let mut states = self.inner.states.lock().unwrap();
        let state = states.entry(peer_id.to_string()).or_default();
        if state.task_id != task_id {
            return;
        }
        state.pending_reconnect = false;
        state.reconnect_attempts = 0;
        drop(states);
        self.set_online(peer_id, true);
    }

    fn connection_closed(&self, peer_id: &str, task_id: u64) {
        let should_reconnect = {
            let mut states = self.inner.states.lock().unwrap();
            let state = states.entry(peer_id.to_string()).or_default();
            if state.task_id != task_id {
                return;
            }
            state.task = None;
            state.online = false;
            self.inner.started.load(Ordering::SeqCst)
        };
        self.emit_peer_status(peer_id, false);
        if should_reconnect {
            self.schedule_reconnect(peer_id.to_string());
        }
    }

    fn schedule_reconnect(&self, peer_id: String) {
        if !self.inner.started.load(Ordering::SeqCst) {
            return;
        }
        let delay_ms = {
            let mut states = self.inner.states.lock().unwrap();
            let state = states.entry(peer_id.clone()).or_default();
            if state.task.is_some() || state.pending_reconnect {
                return;
            }
            state.pending_reconnect = true;
            state.reconnect_attempts = state.reconnect_attempts.saturating_add(1);
            let shift = std::cmp::min(state.reconnect_attempts.saturating_sub(1), 6);
            std::cmp::min(
                MAX_RECONNECT_DELAY_MS,
                INITIAL_RECONNECT_DELAY_MS * (1u64 << shift),
            )
        };
        let manager = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(delay_ms));
            manager.reconnect_peer(&peer_id, "backoff");
        });
    }

    fn load_connectable_peers(&self) -> Vec<DPeer> {
        self.inner
            .db
            .get_peers()
            .into_iter()
            .filter(|peer| self.should_connect(peer))
            .collect()
    }

    fn should_connect(&self, peer: &DPeer) -> bool {
        peer.is_paired() && !peer.key.is_empty() && self.inner.identity.client_id < peer.id
    }

    pub fn set_online(&self, peer_id: &str, online: bool) {
        let changed = {
            let mut states = self.inner.states.lock().unwrap();
            let state = states.entry(peer_id.to_string()).or_default();
            if state.online == online {
                false
            } else {
                state.online = online;
                true
            }
        };
        if changed {
            self.emit_peer_status(peer_id, online);
        }
    }

    fn emit_peer_status(&self, peer_id: &str, online: bool) {
        let payload = serde_json::json!({
            "id": peer_id,
            "online": online,
        })
        .to_string();
        if let Some(event_tx) = self.inner.event_tx.read().unwrap().clone() {
            let _ = event_tx.send(WsEvent::broadcast(WS_PEER_STATUS_UPDATED, payload));
        }
    }
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
