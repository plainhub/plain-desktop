use std::net::TcpListener as StdTcpListener;

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn bind_listener(port: u16) -> std::io::Result<StdTcpListener> {
    let loopback_probe = StdTcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))?;
    let port = loopback_probe.local_addr()?.port();
    let wildcard = StdTcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, port))?;
    drop(loopback_probe);
    Ok(wildcard)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn bind_listener(port: u16) -> std::io::Result<StdTcpListener> {
    StdTcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, port))
}

fn retryable_bind_error(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::AddrInUse | std::io::ErrorKind::PermissionDenied
    )
}

pub const HTTP_PORTS: [u16; 10] = [8080, 8180, 8280, 8380, 8480, 8580, 8680, 8780, 8880, 8980];

pub const HTTPS_PORTS: [u16; 10] = [8043, 8143, 8243, 8343, 8443, 8543, 8643, 8743, 8843, 8943];

pub fn bind_listener_fallback(
    preferred: u16,
    candidates: &[u16],
) -> std::io::Result<StdTcpListener> {
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
#[path = "../../tests/unit/http_transport/bind.rs"]
mod tests;
