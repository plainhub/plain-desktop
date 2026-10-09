use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::net::UdpSocket;
use tokio::sync::RwLock;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Duration;

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
    command_rx: StdMutex<Option<mpsc::UnboundedReceiver<DlnaCommand>>>,
    tasks: StdMutex<Vec<JoinHandle<()>>>,
    stop: tokio::sync::watch::Sender<bool>,
    changes: tokio::sync::broadcast::Sender<()>,
    lifecycle: tokio::sync::Mutex<()>,
    device_uuid: String,
    running: Arc<AtomicBool>,
}

impl DlnaEngine {
    pub fn new() -> Self {
        let (command_tx, command_rx) = mpsc::unbounded_channel::<DlnaCommand>();
        Self {
            state: Arc::new(RwLock::new(DlnaRendererState::default())),
            command_tx: StdMutex::new(Some(command_tx)),
            command_rx: StdMutex::new(Some(command_rx)),
            tasks: StdMutex::new(Vec::new()),
            stop: tokio::sync::watch::channel(false).0,
            changes: tokio::sync::broadcast::channel(32).0,
            lifecycle: tokio::sync::Mutex::new(()),
            device_uuid: generate_uuid(),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn changes(&self) -> tokio::sync::broadcast::Receiver<()> {
        self.changes.subscribe()
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

    /// Current renderer state, for the host UI to project.
    pub async fn snapshot(&self) -> DlnaRendererState {
        let mut state = self.state.write().await;
        state.version += 1;
        state.clone()
    }

    /// The player reports its own position so `GetPositionInfo` answers with
    /// real numbers instead of zeros.
    pub async fn set_position(&self, position_ms: i64, duration_ms: i64) {
        let mut s = self.state.write().await;
        s.current_position_ms = position_ms.max(0);
        s.duration_ms = duration_ms.max(0);
    }

    /// The player has applied `seek_target_ms`; stop re-reporting it.
    pub async fn clear_seek_target(&self) {
        self.state.write().await.seek_target_ms = None;
        let _ = self.changes.send(());
    }

    /// The player reports what it is actually doing, so `GetTransportInfo`
    /// answers with the renderer's real state rather than the last command.
    pub async fn set_playback_state(&self, playback: DlnaPlaybackState) {
        self.state.write().await.playback_state = playback;
        let _ = self.changes.send(());
    }

    /// The player exited: drop the media and go back to no-media-present,
    /// which is what the `Stop` command does.
    pub async fn clear_media(&self) {
        let mut s = self.state.write().await;
        s.media_uri.clear();
        s.media_title.clear();
        s.media_album_art_uri.clear();
        s.media_type = crate::dlna_receiver::types::DlnaMediaType::UNKNOWN;
        s.playback_state = DlnaPlaybackState::NoMediaPresent;
        s.seek_target_ms = None;
        drop(s);
        let _ = self.changes.send(());
    }

    pub async fn set_retrying(&self, retrying: bool) {
        self.state.write().await.is_retrying = retrying;
        let _ = self.changes.send(());
    }

    pub async fn set_start_error(&self, error: String) {
        self.state.write().await.start_error = error;
        let _ = self.changes.send(());
    }

    pub async fn start(&self, port: u16) {
        let _lifecycle = self.lifecycle.lock().await;
        if self.running.swap(true, Ordering::Relaxed) {
            return;
        }
        let Some(socket) = bind_ssdp_socket().await else {
            self.running.store(false, Ordering::Relaxed);
            let mut state = self.state.write().await;
            state.is_running = false;
            state.start_error = "Unable to bind DLNA SSDP socket".into();
            drop(state);
            let _ = self.changes.send(());
            return;
        };
        self.stop.send_replace(false);
        {
            let mut s = self.state.write().await;
            s.start_error.clear();
            s.port = port;
            s.is_running = true;
        }

        if self.command_rx.lock().unwrap().is_none() {
            let previous = std::mem::take(&mut *self.tasks.lock().unwrap());
            for task in previous {
                task.abort();
            }
            let (tx, rx) = mpsc::unbounded_channel();
            *self.command_tx.lock().unwrap() = Some(tx);
            *self.command_rx.lock().unwrap() = Some(rx);
        }
        let mut rx = self.command_rx.lock().unwrap().take().unwrap();
        while rx.try_recv().is_ok() {}

        let state = self.state.clone();
        let task = tokio::spawn(run_command_processor(state, rx, self.changes.clone()));
        self.tasks.lock().unwrap().push(task);

        let state = self.state.clone();
        let uuid = self.device_uuid.clone();
        let running = self.running.clone();
        let stop = self.stop.subscribe();
        let changes = self.changes.clone();
        let task = tokio::spawn(async move {
            run_ssdp_loop(state, &uuid, running, port, socket, stop, changes).await;
        });
        self.tasks.lock().unwrap().push(task);

        let _ = self.changes.send(());
        log::info!(
            "DlnaReceiverEngine started, port={port} uuid={}",
            self.device_uuid
        );
    }

    pub async fn stop(&self) {
        let _lifecycle = self.lifecycle.lock().await;
        self.running.store(false, Ordering::Relaxed);
        self.stop.send_replace(true);
        let tasks = std::mem::take(&mut *self.tasks.lock().unwrap());
        for (index, mut task) in tasks.into_iter().enumerate() {
            if index == 0 {
                task.abort();
            }
            if tokio::time::timeout(Duration::from_secs(1), &mut task)
                .await
                .is_err()
            {
                task.abort();
            }
        }
        let (tx, rx) = mpsc::unbounded_channel();
        *self.command_tx.lock().unwrap() = Some(tx);
        *self.command_rx.lock().unwrap() = Some(rx);
        let mut s = self.state.write().await;
        s.is_running = false;
        s.reset();
        drop(s);
        let _ = self.changes.send(());
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
        self.dispatch_accept(&pending, play_queued).await;
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

    async fn dispatch_accept(&self, pending: &PendingCastRequest, play_queued: bool) {
        accept_pending(&mut *self.state.write().await, pending, play_queued);
        let _ = self.changes.send(());
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
) {
    let s = state.read().await;
    let Some(pending) = s.raw_pending_cast_request.clone() else {
        return;
    };
    let play_queued = s.pending_play_queued;
    drop(s);
    if super::senders_contain_ip(allowed, &pending.sender_ip) {
        accept_pending(&mut *state.write().await, &pending, play_queued);
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
    changes: tokio::sync::broadcast::Sender<()>,
) {
    while let Some(cmd) = rx.recv().await {
        let mut s = state.write().await;
        apply_command(&mut s, cmd);
        drop(s);
        let _ = changes.send(());
    }
}

async fn run_ssdp_loop(
    state: Arc<RwLock<DlnaRendererState>>,
    uuid: &str,
    running: Arc<AtomicBool>,
    port: u16,
    socket: UdpSocket,
    mut stop: tokio::sync::watch::Receiver<bool>,
    changes: tokio::sync::broadcast::Sender<()>,
) {
    let ip = local_ip();
    for msg in ssdp_messages::alive_messages(uuid, &ip, port) {
        let _ = socket.send_to(msg.as_bytes(), (SSDP_ADDR, SSDP_PORT)).await;
    }
    log::info!("DLNA SSDP advertiser started, sent initial alive");

    let mut buf = [0u8; 2048];
    while running.load(Ordering::Relaxed) {
        let packet = tokio::select! {
            _ = stop.changed() => break,
            packet = tokio::time::timeout(Duration::from_secs(30), socket.recv_from(&mut buf)) => packet,
        };
        match packet {
            Ok(Ok((n, src))) => {
                let msg = String::from_utf8_lossy(&buf[..n]);
                respond_to_search(&socket, &msg, src, uuid, &local_ip(), port).await;
            }
            Ok(Err(e)) => {
                log::error!("DLNA SSDP receive error: {e}");
                running.store(false, Ordering::Relaxed);
                let mut state = state.write().await;
                state.is_running = false;
                state.start_error = e.to_string();
                drop(state);
                let _ = changes.send(());
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

async fn respond_to_search(
    socket: &UdpSocket,
    message: &str,
    source: std::net::SocketAddr,
    uuid: &str,
    ip: &str,
    port: u16,
) {
    if message.contains("M-SEARCH") {
        for response in ssdp_messages::search_responses(uuid, ip, port) {
            let _ = socket.send_to(response.as_bytes(), source).await;
        }
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

fn accept_pending(state: &mut DlnaRendererState, pending: &PendingCastRequest, play_queued: bool) {
    state.pending_cast_request = None;
    state.raw_pending_cast_request = None;
    state.pending_play_queued = false;
    apply_command(
        state,
        DlnaCommand::SetUri {
            uri: pending.media_uri.clone(),
            title: pending.media_title.clone(),
            media_type: pending.media_type,
            album_art_uri: pending.album_art_uri.clone(),
        },
    );
    if play_queued {
        apply_command(state, DlnaCommand::Play);
    }
}

fn apply_command(s: &mut DlnaRendererState, cmd: DlnaCommand) {
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

#[cfg(test)]
#[path = "../../tests/unit/dlna_receiver/receiver_engine.rs"]
mod tests;
