use axum_server::tls_rustls::RustlsConfig;
use std::net::TcpListener as StdTcpListener;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use super::ServerState;

struct ServerHandle {
    http_task: tokio::task::JoinHandle<()>,
    https_task: Option<tokio::task::JoinHandle<()>>,
}

pub struct ServerRuntime {
    state: ServerState,
    rustls: Option<RustlsConfig>,
    server: Mutex<ServerHandle>,
}

impl ServerRuntime {
    pub async fn start(state: ServerState) -> Self {
        let rustls = match crate::server::tls::ensure_cert(&state.ctx.data_dir) {
            Ok((cert_pem, key_pem)) => match RustlsConfig::from_pem(cert_pem, key_pem).await {
                Ok(config) => Some(config),
                Err(error) => {
                    log::error!("local_server: failed to build rustls config: {error}");
                    None
                }
            },
            Err(error) => {
                log::error!("local_server: failed to ensure cert: {error}");
                None
            }
        };
        let runtime = Self {
            state,
            rustls,
            server: Mutex::new(ServerHandle {
                http_task: tokio::spawn(async {}),
                https_task: None,
            }),
        };
        runtime.rebind().await.expect("initial local server bind");
        runtime
    }

    async fn rebind(&self) -> Result<(), String> {
        let ctx = &self.state.ctx;
        let old_http = ctx.port.load(Ordering::Relaxed);
        let old_https = ctx.https_port.load(Ordering::Relaxed);
        let http_port = crate::prefs::server::http_port(&ctx.prefs);
        let https_port = crate::prefs::server::https_port(&ctx.prefs);

        {
            let mut server = self.server.lock().unwrap();
            server.http_task.abort();
            if let Some(task) = server.https_task.take() {
                task.abort();
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let http_listener = match bind_listener_fallback(http_port, &HTTP_PORTS) {
            Ok(listener) => listener,
            Err(error) => {
                if old_http != 0 {
                    crate::prefs::server::set_http_port(&ctx.prefs, old_http);
                }
                return Err(format!("HTTP port {http_port} bind failed: {error}"));
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
            Ok(listener) => listener,
            Err(error) => {
                drop(http_listener);
                if old_http != 0 {
                    crate::prefs::server::set_http_port(&ctx.prefs, old_http);
                }
                if old_https != 0 {
                    crate::prefs::server::set_https_port(&ctx.prefs, old_https);
                }
                return Err(format!("HTTPS port {https_port} bind failed: {error}"));
            }
        };
        let new_https_port = https_listener.local_addr().expect("https addr").port();
        https_listener
            .set_nonblocking(true)
            .expect("set_nonblocking https");

        ctx.port.store(new_port, Ordering::Relaxed);
        ctx.https_port.store(new_https_port, Ordering::Relaxed);

        let http_app = super::build_router(self.state.clone())
            .into_make_service_with_connect_info::<std::net::SocketAddr>();
        let http_task = tokio::spawn(async move {
            let listener = tokio::net::TcpListener::from_std(http_listener).expect("http listener");
            if let Err(error) = axum::serve(listener, http_app).await {
                log::error!("local_server: http serve error: {error}");
            }
        });

        let https_task = self.rustls.clone().map(|config| {
            let https_app = super::build_router(self.state.clone())
                .into_make_service_with_connect_info::<std::net::SocketAddr>();
            tokio::spawn(async move {
                if let Err(error) = axum_server::from_tcp_rustls(https_listener, config)
                    .serve(https_app)
                    .await
                {
                    log::error!("local_server: https serve error: {error}");
                }
            })
        });

        let mut server = self.server.lock().unwrap();
        server.http_task = http_task;
        server.https_task = https_task;
        ctx.discover_manager.set_https_port(new_https_port);
        Ok(())
    }

    pub async fn restart(&self) -> Result<(), String> {
        self.rebind().await
    }

    pub fn port(&self) -> u16 {
        self.state.ctx.port.load(Ordering::Relaxed)
    }

    pub fn https_port(&self) -> u16 {
        self.state.ctx.https_port.load(Ordering::Relaxed)
    }

    pub fn token(&self) -> &str {
        &self.state.ctx.token
    }
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
