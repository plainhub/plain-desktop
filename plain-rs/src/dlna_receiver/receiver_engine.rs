use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::net::UdpSocket;
use tokio::sync::RwLock;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{Duration, sleep};

use crate::dlna_receiver::renderer_state::DlnaRendererState;
use crate::dlna_receiver::ssdp_messages;
use crate::dlna_receiver::types::{DlnaCommand, DlnaPlaybackState, PendingCastRequest};
use crate::mdns::host_responder::local_ipv4_strs;

const SSDP_ADDR: &str = "239.255.255.250";
const SSDP_PORT: u16 = 1900;

/// Owns the DLNA MediaRenderer receiver: SSDP advertiser, command processing,
/// and the UPnP control endpoints (routed via [`Self::route`]). Mirrors
/// plain-app's `DlnaReceiverEngine` + `DlnaRendererState`.
pub struct DlnaEngine {
    pub state: Arc<RwLock<DlnaRendererState>>,
    command_tx: StdMutex<Option<mpsc::UnboundedSender<DlnaCommand>>>,
    tasks: StdMutex<Vec<JoinHandle<()>>>,
    device_uuid: String,
    running: Arc<AtomicBool>,
}

impl DlnaEngine {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(DlnaRendererState::default())),
            command_tx: StdMutex::new(None),
            tasks: StdMutex::new(Vec::new()),
            device_uuid: generate_uuid(),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn device_uuid(&self) -> &str {
        &self.device_uuid
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    #[allow(dead_code)]
    pub fn send_command(&self, cmd: DlnaCommand) {
        if let Some(tx) = self.command_tx.lock().unwrap().as_ref() {
            let _ = tx.send(cmd);
        }
    }

    pub fn command_sender(&self) -> Option<mpsc::UnboundedSender<DlnaCommand>> {
        self.command_tx.lock().unwrap().clone()
    }

    pub async fn start(&self, port: u16) {
        if self.running.swap(true, Ordering::Relaxed) {
            return;
        }
        {
            let mut s = self.state.write().await;
            s.start_error.clear();
            s.port = port;
            s.is_running = true;
        }

        let (tx, rx) = mpsc::unbounded_channel::<DlnaCommand>();
        *self.command_tx.lock().unwrap() = Some(tx);

        let state = self.state.clone();
        let task = tokio::spawn(run_command_processor(state, rx));
        self.tasks.lock().unwrap().push(task);

        let state = self.state.clone();
        let uuid = self.device_uuid.clone();
        let running = self.running.clone();
        let task = tokio::spawn(async move {
            run_ssdp_loop(state, &uuid, running, port).await;
        });
        self.tasks.lock().unwrap().push(task);

        log::info!(
            "DlnaReceiverEngine started, port={port} uuid={}",
            self.device_uuid
        );
    }

    pub async fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
        for t in self.tasks.lock().unwrap().iter() {
            t.abort();
        }
        self.tasks.lock().unwrap().clear();
        *self.command_tx.lock().unwrap() = None;
        let mut s = self.state.write().await;
        s.is_running = false;
        s.reset();
        log::info!("DlnaReceiverEngine stopped");
    }

    /// Accept the current pending cast request. Mirrors plain-app's
    /// `DlnaRendererState.acceptCastRequest`: dispatches SetUri (and a queued
    /// Play), clears pending state, and optionally persists the sender as
    /// allowed so future requests are auto-accepted.
    pub async fn accept_cast(&self, remember: bool, prefs: &crate::prefs::Prefs) {
        let s = self.state.read().await;
        let Some(pending) = s.pending_cast_request.clone() else {
            return;
        };
        let play_queued = s.pending_play_queued;
        drop(s);
        self.dispatch_accept(&pending, play_queued);
        if remember && !pending.sender_ip.is_empty() {
            crate::prefs::dlna::remove_sender(prefs, "dlna_denied_senders", &pending.sender_ip);
            crate::prefs::dlna::add_sender(
                prefs,
                "dlna_allowed_senders",
                &pending.sender_ip,
                &pending.sender_name,
            );
        }
    }

    /// Reject the current pending cast request. Mirrors plain-app's
    /// `DlnaRendererState.rejectCastRequest`: clears pending state and
    /// optionally persists the sender as denied.
    pub async fn reject_cast(&self, remember: bool, prefs: &crate::prefs::Prefs) {
        let s = self.state.read().await;
        let Some(pending) = s.pending_cast_request.clone() else {
            return;
        };
        drop(s);
        let mut s = self.state.write().await;
        s.pending_cast_request = None;
        s.pending_play_queued = false;
        drop(s);
        if remember && !pending.sender_ip.is_empty() {
            crate::prefs::dlna::remove_sender(prefs, "dlna_allowed_senders", &pending.sender_ip);
            crate::prefs::dlna::add_sender(
                prefs,
                "dlna_denied_senders",
                &pending.sender_ip,
                &pending.sender_name,
            );
        }
    }

    fn dispatch_accept(&self, pending: &PendingCastRequest, play_queued: bool) {
        let mut s = self.state.blocking_write();
        s.pending_cast_request = None;
        s.raw_pending_cast_request = None;
        s.pending_play_queued = false;
        s.media_uri = pending.media_uri.clone();
        s.media_title = pending.media_title.clone();
        s.media_album_art_uri = pending.album_art_uri.clone();
        s.media_type = pending.media_type;
        s.playback_state = DlnaPlaybackState::Transitioning;
        if play_queued {
            s.playback_state = DlnaPlaybackState::Playing;
        }
        drop(s);
    }
}

/// Inspect the raw pending cast request and apply the allow/deny rules.
/// Mirrors plain-app's `DlnaReceiverEngine.startRuleCheck` — but runs
/// synchronously inside the HTTP route handler (the only writer of
/// `raw_pending_cast_request`) instead of a polling coroutine.
pub async fn check_rules(
    state: &Arc<RwLock<DlnaRendererState>>,
    allowed: &[String],
    denied: &[String],
    command_tx: &mpsc::UnboundedSender<DlnaCommand>,
) {
    let s = state.read().await;
    let Some(pending) = s.raw_pending_cast_request.clone() else {
        return;
    };
    let play_queued = s.pending_play_queued;
    drop(s);
    if super::senders_contain_ip(allowed, &pending.sender_ip) {
        let mut s = state.write().await;
        s.raw_pending_cast_request = None;
        s.pending_cast_request = None;
        s.pending_play_queued = false;
        s.media_uri = pending.media_uri.clone();
        s.media_title = pending.media_title.clone();
        s.media_album_art_uri = pending.album_art_uri.clone();
        s.media_type = pending.media_type;
        s.playback_state = DlnaPlaybackState::Transitioning;
        if play_queued {
            s.playback_state = DlnaPlaybackState::Playing;
        }
        drop(s);
        let _ = command_tx.send(DlnaCommand::SetUri {
            uri: pending.media_uri,
            title: pending.media_title,
            media_type: pending.media_type,
            album_art_uri: pending.album_art_uri,
        });
        if play_queued {
            let _ = command_tx.send(DlnaCommand::Play);
        }
    } else if super::senders_contain_ip(denied, &pending.sender_ip) {
        let mut s = state.write().await;
        s.raw_pending_cast_request = None;
        s.pending_play_queued = false;
        drop(s);
    } else {
        let mut s = state.write().await;
        s.pending_cast_request = Some(pending);
        s.raw_pending_cast_request = None;
        drop(s);
    }
}

async fn run_command_processor(
    state: Arc<RwLock<DlnaRendererState>>,
    mut rx: mpsc::UnboundedReceiver<DlnaCommand>,
) {
    while let Some(cmd) = rx.recv().await {
        let mut s = state.write().await;
        match cmd {
            DlnaCommand::SetUri {
                uri,
                title,
                media_type,
                album_art_uri,
            } => {
                s.media_uri = uri;
                s.media_title = title;
                s.media_album_art_uri = album_art_uri;
                s.media_type = media_type;
                s.playback_state = DlnaPlaybackState::Transitioning;
            }
            DlnaCommand::Play => s.playback_state = DlnaPlaybackState::Playing,
            DlnaCommand::Pause => s.playback_state = DlnaPlaybackState::PausedPlayback,
            DlnaCommand::Stop => {
                s.seek_target_ms = Some(0);
                s.media_uri.clear();
                s.playback_state = DlnaPlaybackState::NoMediaPresent;
            }
            DlnaCommand::Seek { position_ms } => s.seek_target_ms = Some(position_ms),
        }
    }
}

async fn run_ssdp_loop(
    _state: Arc<RwLock<DlnaRendererState>>,
    uuid: &str,
    running: Arc<AtomicBool>,
    port: u16,
) {
    let Some(socket) = bind_ssdp_socket().await else {
        log::warn!("DLNA SSDP socket unavailable — discovery disabled");
        while running.load(Ordering::Relaxed) {
            sleep(Duration::from_secs(1)).await;
        }
        return;
    };

    let ip = local_ip();
    for msg in ssdp_messages::alive_messages(uuid, &ip, port) {
        let _ = socket.send_to(msg.as_bytes(), (SSDP_ADDR, SSDP_PORT)).await;
    }
    log::info!("DLNA SSDP advertiser started, sent initial alive");

    let mut buf = [0u8; 2048];
    while running.load(Ordering::Relaxed) {
        match tokio::time::timeout(Duration::from_secs(30), socket.recv_from(&mut buf)).await {
            Ok(Ok((n, src))) => {
                let msg = String::from_utf8_lossy(&buf[..n]);
                if msg.contains("M-SEARCH") {
                    let ip = local_ip();
                    for resp in ssdp_messages::search_responses(uuid, &ip, port) {
                        let _ = socket.send_to(resp.as_bytes(), src).await;
                    }
                }
            }
            Ok(Err(e)) => {
                log::error!("DLNA SSDP receive error: {e}");
                break;
            }
            Err(_) => {
                if !running.load(Ordering::Relaxed) {
                    break;
                }
                let ip = local_ip();
                for msg in ssdp_messages::alive_messages(uuid, &ip, port) {
                    let _ = socket.send_to(msg.as_bytes(), (SSDP_ADDR, SSDP_PORT)).await;
                }
            }
        }
    }

    let ip = local_ip();
    for msg in ssdp_messages::byebye_messages(uuid, &ip, port) {
        let _ = socket.send_to(msg.as_bytes(), (SSDP_ADDR, SSDP_PORT)).await;
    }
}

async fn bind_ssdp_socket() -> Option<UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};

    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).ok()?;
    socket.set_reuse_address(true).ok()?;
    socket.set_nonblocking(true).ok()?;
    let addr: std::net::SocketAddr = format!("0.0.0.0:{SSDP_PORT}").parse().ok()?;
    socket.bind(&addr.into()).ok()?;
    let multi: std::net::Ipv4Addr = SSDP_ADDR.parse().ok()?;
    let any: std::net::Ipv4Addr = "0.0.0.0".parse().ok()?;
    socket.join_multicast_v4(&multi, &any).ok()?;
    let std_socket: std::net::UdpSocket = socket.into();
    UdpSocket::from_std(std_socket).ok()
}

fn local_ip() -> String {
    local_ipv4_strs()
        .into_iter()
        .next()
        .unwrap_or_else(|| "127.0.0.1".to_string())
}

fn generate_uuid() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; 16];
    rng.fill(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}
