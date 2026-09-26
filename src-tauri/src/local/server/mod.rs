//! Local HTTP+HTTPS+WebSocket server for offline/local mode.
//!
//! - HTTP — plain requests and WebSocket upgrades.
//! - HTTPS — encrypted requests and WSS upgrades.
//!
//! Each listener binds the user-configured port stored in `prefs` (default
//! 8080 / 8443, matching plain-app's `HttpPortPreference` / `HttpsPortPreference`).
//! On conflict it walks the candidate lists [`HTTP_PORTS`] / [`HTTPS_PORTS`]
//! starting from the configured port and binds the first free one. If every
//! fixed candidate is occupied, it asks the OS for a free port so the app can
//! keep running. The bound port is recorded in `LocalServerState` so downstream
//! consumers (pairing, discovery, GraphQL) see the actual value.
//!
//! Per-connection dispatch lives in [`plain_conn`] (HTTP/WS) and
//! [`tls_conn`] (HTTPS/WSS); the listener loops here only accept and spawn.

use super::chat::ChatState;
use super::db::ChatDb;
use super::graphql::{AppCtx, LocalSchema, WsEvent, build_schema};
use super::peer_graphql::{PeerSchema, build_schema as build_peer_schema};
use super::tls::{build_acceptor, ensure_cert};
use crate::commands::discover::{NearbyDiscoverManager, PeerStatusManager};
use crate::prefs::AppIdentity;
use std::net::TcpListener as StdTcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use tauri::{AppHandle, Manager};
use tokio::sync::broadcast;
use tokio_rustls::TlsAcceptor;

mod file_server;
mod http_handler;
mod plain_conn;
mod proxy_file;
pub(super) mod response;
mod tls_conn;
mod upload;
pub(crate) mod uri;
mod ws_handler;

struct ServerHandle {
    http_task: tauri::async_runtime::JoinHandle<()>,
    https_task: Option<tauri::async_runtime::JoinHandle<()>>,
}

pub struct LocalServerState {
    pub token: String,
    pub event_tx: broadcast::Sender<WsEvent>,
    port: Arc<AtomicU16>,
    https_port: Arc<AtomicU16>,
    schema: Arc<LocalSchema>,
    peer_schema: Arc<PeerSchema>,
    ctx: Arc<AppCtx>,
    acceptor: Option<Arc<TlsAcceptor>>,
    data_dir: PathBuf,
    handle: AppHandle,
    server: Mutex<ServerHandle>,
}

impl LocalServerState {
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        app_data_dir: PathBuf,
        log_dir: PathBuf,
        db: Arc<ChatDb>,
        library: Arc<crate::local::db::LibraryDb>,
        handle: AppHandle,
        identity: Arc<AppIdentity>,
        device_name: Arc<RwLock<String>>,
        chat: Arc<ChatState>,
        peer_status: PeerStatusManager,
        discover_manager: NearbyDiscoverManager,
        dlna_engine: Arc<crate::local::dlna::receiver_engine::DlnaEngine>,
    ) -> Self {
        let port = Arc::new(AtomicU16::new(0));
        let https_port = Arc::new(AtomicU16::new(0));

        let token = crate::prefs::get_url_token(&handle);

        let (event_tx, _) = broadcast::channel::<WsEvent>(1024);

        // Fan chat + pairing broadcast events out to the local-server WS
        // bus and the Tauri `pairing-event`.
        chat.spawn_event_bridges(event_tx.clone(), handle.clone());

        let schema = Arc::new(build_schema());
        let peer_schema: Arc<PeerSchema> = Arc::new(build_peer_schema());
        let ctx = Arc::new(AppCtx {
            db: db.clone(),
            library,
            identity: identity.clone(),
            peer_status: peer_status.clone(),
            discover_manager: discover_manager.clone(),
            chat: chat.clone(),
            dlna_engine: dlna_engine.clone(),
            event_tx: event_tx.clone(),
            token: token.clone(),
            port: port.clone(),
            https_port: https_port.clone(),
            data_dir: app_data_dir.clone(),
            log_dir,
            device_name,
            handle: handle.clone(),
        });

        let acceptor: Option<Arc<TlsAcceptor>> = match ensure_cert(&app_data_dir) {
            Ok((cert_pem, key_pem)) => match build_acceptor(&cert_pem, &key_pem) {
                Ok(a) => Some(Arc::new(a)),
                Err(e) => {
                    log::error!("local_server: failed to build TLS acceptor: {e}");
                    None
                }
            },
            Err(e) => {
                log::error!("local_server: failed to ensure cert: {e}");
                None
            }
        };

        let state = LocalServerState {
            token: token.clone(),
            event_tx,
            port,
            https_port,
            schema,
            peer_schema,
            ctx,
            acceptor,
            data_dir: app_data_dir,
            handle: handle.clone(),
            server: Mutex::new(ServerHandle {
                http_task: tauri::async_runtime::spawn(async {}),
                https_task: None,
            }),
        };
        state.rebind().expect("initial local server bind");
        state
    }

    fn rebind(&self) -> Result<(), String> {
        let old_http = self.port.load(Ordering::Relaxed);
        let old_https = self.https_port.load(Ordering::Relaxed);
        let http_port = crate::prefs::get_http_port(&self.handle);
        let https_port = crate::prefs::get_https_port(&self.handle);

        {
            let mut server = self.server.lock().unwrap();
            server.http_task.abort();
            if let Some(t) = server.https_task.take() {
                t.abort();
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));

        let http_listener = match bind_listener_fallback(http_port, &HTTP_PORTS) {
            Ok(l) => l,
            Err(e) => {
                if old_http != 0 {
                    crate::prefs::set_http_port(&self.handle, old_http);
                }
                return Err(format!("HTTP port {http_port} bind failed: {e}"));
            }
        };
        let new_port = http_listener
            .local_addr()
            .expect("local server addr")
            .port();
        http_listener
            .set_nonblocking(true)
            .expect("set_nonblocking");

        let https_listener = match bind_listener_fallback(https_port, &HTTPS_PORTS) {
            Ok(l) => l,
            Err(e) => {
                drop(http_listener);
                if old_http != 0 {
                    crate::prefs::set_http_port(&self.handle, old_http);
                }
                if old_https != 0 {
                    crate::prefs::set_https_port(&self.handle, old_https);
                }
                return Err(format!("HTTPS port {https_port} bind failed: {e}"));
            }
        };
        let new_https_port = https_listener.local_addr().expect("https addr").port();
        https_listener
            .set_nonblocking(true)
            .expect("set_nonblocking https");

        self.port.store(new_port, Ordering::Relaxed);
        self.https_port.store(new_https_port, Ordering::Relaxed);

        let token_arc = Arc::new(self.token.clone());
        let http_task = {
            let schema = self.schema.clone();
            let peer_schema = self.peer_schema.clone();
            let ctx = self.ctx.clone();
            let event_tx = self.event_tx.clone();
            let data_dir = self.data_dir.clone();
            tauri::async_runtime::spawn(async move {
                let listener =
                    tokio::net::TcpListener::from_std(http_listener).expect("http listener");
                loop {
                    let Ok((stream, _)) = listener.accept().await else {
                        continue;
                    };
                    let _ = stream.set_nodelay(true);
                    tokio::spawn(plain_conn::serve(
                        stream,
                        schema.clone(),
                        peer_schema.clone(),
                        ctx.clone(),
                        token_arc.clone(),
                        event_tx.subscribe(),
                        data_dir.clone(),
                    ));
                }
            })
        };

        let https_task = self.acceptor.as_ref().map(|acc| {
            let schema = self.schema.clone();
            let peer_schema = self.peer_schema.clone();
            let ctx = self.ctx.clone();
            let data_dir = self.data_dir.clone();
            let acc = acc.clone();
            tauri::async_runtime::spawn(async move {
                let listener =
                    tokio::net::TcpListener::from_std(https_listener).expect("https listener");
                loop {
                    let Ok((stream, _)) = listener.accept().await else {
                        continue;
                    };
                    let _ = stream.set_nodelay(true);
                    tokio::spawn(tls_conn::serve(
                        stream,
                        acc.clone(),
                        schema.clone(),
                        peer_schema.clone(),
                        ctx.clone(),
                        data_dir.clone(),
                    ));
                }
            })
        });

        let mut server = self.server.lock().unwrap();
        server.http_task = http_task;
        server.https_task = https_task;

        // Publish the `_plainapp._tcp.local` mDNS service with the bound
        // HTTPS port — mirrors plain-app's `NsdHelper.registerService` being
        // driven by the HTTP service lifecycle.
        self.ctx.discover_manager.set_https_port(new_https_port);
        Ok(())
    }

    pub fn restart(&self) -> Result<(), String> {
        self.rebind()
    }

    pub fn port(&self) -> u16 {
        self.port.load(Ordering::Relaxed)
    }

    pub fn https_port(&self) -> u16 {
        self.https_port.load(Ordering::Relaxed)
    }
}

// ── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn local_server_port(state: tauri::State<'_, LocalServerState>) -> u16 {
    state.port()
}

#[tauri::command]
pub fn local_server_https_port(state: tauri::State<'_, LocalServerState>) -> u16 {
    state.https_port()
}

#[tauri::command]
pub fn local_server_token(state: tauri::State<'_, LocalServerState>) -> String {
    state.token.clone()
}

#[tauri::command]
pub async fn local_ipv4_strs() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(crate::commands::discover::discover_local_ipv4_strs)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_http_port(handle: tauri::AppHandle, port: u16) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || crate::prefs::set_http_port(&handle, port))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_https_port(handle: tauri::AppHandle, port: u16) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || crate::prefs::set_https_port(&handle, port))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn restart_server(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<LocalServerState>();
        state.restart()
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── TCP listener binding ──────────────────────────────────────────────────────

/// On macOS and Windows, hold a loopback probe while acquiring the wildcard
/// listener to catch address-coexistence quirks: both platforms let a wildcard
/// bind succeed alongside an already-bound loopback address on the same port,
/// leaving 127.0.0.1 traffic going to the other socket. Keeping the probe
/// alive also makes an OS-assigned port request use the same port for both
/// binds.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn bind_listener(port: u16) -> std::io::Result<StdTcpListener> {
    let loopback_probe = StdTcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))?;
    let port = loopback_probe.local_addr()?.port();
    let wildcard = StdTcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, port))?;
    drop(loopback_probe);
    Ok(wildcard)
}

/// On other platforms, the wildcard listener already owns loopback, so a
/// second loopback bind would conflict with our own socket.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn bind_listener(port: u16) -> std::io::Result<StdTcpListener> {
    StdTcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, port))
}

fn retryable_bind_error(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::AddrInUse | std::io::ErrorKind::PermissionDenied
    )
}

/// Candidate HTTP ports tried in order when the configured one is occupied,
/// matching plain-app's mobile port list.
const HTTP_PORTS: [u16; 10] = [8080, 8180, 8280, 8380, 8480, 8580, 8680, 8780, 8880, 8980];

/// Candidate HTTPS ports tried in order when the configured one is occupied.
const HTTPS_PORTS: [u16; 10] = [8043, 8143, 8243, 8343, 8443, 8543, 8643, 8743, 8843, 8943];

/// Bind the configured port; on an address conflict, walk `candidates` starting
/// from the configured port and bind the first free one. If none are available,
/// ask the OS for a free port so startup can continue.
fn bind_listener_fallback(preferred: u16, candidates: &[u16]) -> std::io::Result<StdTcpListener> {
    let preferred_index = candidates.iter().position(|&p| p == preferred);
    if preferred_index.is_none() {
        match bind_listener(preferred) {
            Ok(listener) => return Ok(listener),
            Err(e) if retryable_bind_error(&e) => {
                log::warn!(
                    "local_server: port {preferred} is unavailable, trying fixed candidates"
                );
            }
            Err(e) => return Err(e),
        }
    }

    let start = preferred_index.unwrap_or(0);
    for i in 0..candidates.len() {
        let port = candidates[(start + i) % candidates.len()];
        match bind_listener(port) {
            Ok(l) => return Ok(l),
            Err(e) if retryable_bind_error(&e) => {
                log::warn!("local_server: port {port} is unavailable, trying next candidate");
            }
            Err(e) => return Err(e),
        }
    }
    log::warn!("local_server: all fixed ports are unavailable, asking the OS for a free port");
    bind_listener(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener as StdTcpListener;
    use std::sync::Mutex;

    static SERIAL: Mutex<()> = Mutex::new(());

    fn grab_port() -> (StdTcpListener, u16) {
        let l = StdTcpListener::bind("0.0.0.0:0").expect("grab :0");
        let p = l.local_addr().unwrap().port();
        (l, p)
    }

    fn grab_loopback_port() -> (StdTcpListener, u16) {
        let l = StdTcpListener::bind("127.0.0.1:0").expect("grab loopback :0");
        let p = l.local_addr().unwrap().port();
        (l, p)
    }

    #[test]
    fn bind_listener_succeeds_on_free_port() {
        let _guard = SERIAL.lock().unwrap();
        let (probe_hold, free_port) = grab_port();
        drop(probe_hold);
        let listener = bind_listener(free_port).expect("known-free fixed port should bind");
        assert_eq!(listener.local_addr().unwrap().port(), free_port);
    }

    #[test]
    fn bind_listener_uses_os_assigned_port() {
        let _guard = SERIAL.lock().unwrap();
        let listener = bind_listener(0).expect("bind :0 should always succeed");
        assert_ne!(listener.local_addr().unwrap().port(), 0);
    }

    #[test]
    fn bind_listener_fails_when_wildcard_taken() {
        let _guard = SERIAL.lock().unwrap();
        let (taken_hold, taken) = grab_port();
        let err = bind_listener(taken).unwrap_err();
        assert!(
            err.kind() == std::io::ErrorKind::AddrInUse
                || err.kind() == std::io::ErrorKind::PermissionDenied,
            "expected AddrInUse or PermissionDenied, got {:?}",
            err.kind()
        );
        drop(taken_hold);
    }

    #[test]
    fn bind_listener_fails_when_loopback_taken() {
        let _guard = SERIAL.lock().unwrap();
        let (taken_hold, taken) = grab_loopback_port();
        let err = bind_listener(taken).unwrap_err();
        assert!(
            err.kind() == std::io::ErrorKind::AddrInUse
                || err.kind() == std::io::ErrorKind::PermissionDenied,
            "expected AddrInUse or PermissionDenied, got {:?}",
            err.kind()
        );
        drop(taken_hold);
    }

    #[test]
    fn bind_listener_fallback_succeeds_when_port_taken() {
        let _guard = SERIAL.lock().unwrap();
        let (taken_hold, taken) = grab_port();
        let (free_hold, free) = grab_port();
        drop(free_hold);
        let l = bind_listener_fallback(taken, &[taken, free])
            .expect("fallback should bind the next free candidate");
        assert_eq!(l.local_addr().unwrap().port(), free);
        drop(taken_hold);
    }

    #[test]
    fn bind_listener_fallback_prefers_free_configured_port() {
        let _guard = SERIAL.lock().unwrap();
        let (probe_hold, free_port) = grab_port();
        drop(probe_hold);
        let l = bind_listener_fallback(free_port, &[free_port])
            .expect("a free preferred port should be used");
        assert_eq!(l.local_addr().unwrap().port(), free_port);
    }

    #[test]
    fn bind_listener_fallback_tries_preferred_port_outside_candidates() {
        let _guard = SERIAL.lock().unwrap();
        let (preferred_hold, preferred) = grab_port();
        let (candidate_hold, candidate) = grab_port();
        drop(preferred_hold);
        drop(candidate_hold);
        let listener = bind_listener_fallback(preferred, &[candidate])
            .expect("a free custom preferred port should be used");
        assert_eq!(listener.local_addr().unwrap().port(), preferred);
    }

    #[test]
    fn bind_listener_fallback_uses_os_port_when_all_candidates_taken() {
        let _guard = SERIAL.lock().unwrap();
        let (a, pa) = grab_port();
        let (b, pb) = grab_port();
        let listener = bind_listener_fallback(pa, &[pa, pb])
            .expect("OS-assigned fallback should keep the server running");
        let picked = listener.local_addr().unwrap().port();
        assert_ne!(picked, pa);
        assert_ne!(picked, pb);
        drop(a);
        drop(b);
    }

    #[test]
    fn bind_listener_fallback_handles_empty_candidates() {
        let _guard = SERIAL.lock().unwrap();
        let (preferred_hold, preferred) = grab_port();
        let listener = bind_listener_fallback(preferred, &[])
            .expect("empty candidate list should fall back to an OS-assigned port");
        let picked = listener.local_addr().unwrap().port();
        assert_ne!(picked, 0);
        assert_ne!(picked, preferred);
        drop(preferred_hold);
    }
}
