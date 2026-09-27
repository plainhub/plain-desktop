//! SSDP M-SEARCH send/response loop.
//!
//! Mirrors `internal/dlna/ssdp.go`. We open one UDP socket per local IPv4
//! address (plus one auto-bind), broadcast the M-SEARCH three times with a
//! 60ms gap, then read responses until either the global deadline expires
//! or 250ms has passed since the last response. The result is a list of
//! raw `UpnpDiscovered` records keyed by `Location` URL — callers then
//! fetch the XML descriptor for each unique location.
//!
//! Why not `socket2`?
//! ------------------
//! The `socket2` crate is a fairly direct wrapper around BSD socket
//! operations; it adds little over `std::net::UdpSocket` + a small amount
//! of `libc::setsockopt` glue. We need four socket options:
//!
//!   * `SO_BROADCAST`         — required to send to subnet-directed bcast
//!   * `SO_RCVTIMEO`          — set per-call read deadline
//!   * `SO_SNDBUF` / `SO_RCVBUF` — bump kernel buffers to fit a 5s burst
//!
//! That's three `setsockopt` calls — well under 30 lines of unsafe.
//! Removing `socket2` lets us drop ~30 transitive crates (its `all`
//! features pull in `redox_syscall`, `libc` cfg machinery, etc.) for
//! minimal loss of functionality.

use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::os::fd::AsRawFd;
use std::time::Duration;

use super::types::UpnpDiscovered;
use super::util::{dlna_debug_enabled, dlna_debug_payload_enabled, truncate_for_log};

/// Hard SSDP round deadline (the Go side uses 5s). Even if the caller's
/// context allows longer, the SSDP phase is bounded here.
const SSDP_HARD_DEADLINE: Duration = Duration::from_secs(5);
/// Idle time after the last response before we stop reading.
const IDLE_AFTER_LAST_RESPONSE: Duration = Duration::from_millis(250);
/// Read deadline per `recv_from` call — short so we can check the idle timer.
const CHUNK_READ_DEADLINE: Duration = Duration::from_millis(300);
/// How many times we re-send each M-SEARCH (matches Go's `i < 3` loop).
const M_SEARCH_REPEAT: usize = 3;
/// Gap between M-SEARCH re-sends.
const M_SEARCH_GAP: Duration = Duration::from_millis(60);
/// Read buffer size for a single datagram. SSDP responses are < 1 KiB.
const RECV_BUF: usize = 4096;
/// Bumped kernel buffer size (the spec is < 4 KiB; we ask for 256 KiB
/// to coalesce bursts).
const KERNEL_BUF_BYTES: libc::c_int = 256 * 1024;
/// Multicast IPv4 group address for UPnP.
const SSDP_MCAST_ADDR: Ipv4Addr = Ipv4Addr::new(239, 255, 255, 250);
/// Multicast port.
const SSDP_MCAST_PORT: u16 = 1900;

#[derive(Debug, Clone, Copy)]
pub struct LocalIpv4Addr {
    pub ip: Ipv4Addr,
    pub broadcast: Option<Ipv4Addr>,
}

/// Set a `timeval` socket option. We use this for `SO_RCVTIMEO` and
/// `SO_SNDTIMEO`. `None` clears the timeout (blocking forever).
///
/// SAFETY: caller must pass a valid `fd`.
unsafe fn set_socket_timeout(
    fd: i32,
    optname: libc::c_int,
    dur: Option<Duration>,
) -> std::io::Result<()> {
    let tv = match dur {
        Some(d) => libc::timeval {
            tv_sec: d.as_secs() as libc::time_t,
            tv_usec: d.subsec_micros() as libc::suseconds_t,
        },
        None => libc::timeval {
            tv_sec: 0,
            tv_usec: 0,
        },
    };
    let rc = unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            optname,
            &tv as *const _ as *const libc::c_void,
            std::mem::size_of::<libc::timeval>() as libc::socklen_t,
        )
    };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// Set an integer socket option (`SO_BROADCAST`, `SO_RCVBUF`, `SO_SNDBUF`).
///
/// SAFETY: caller must pass a valid `fd`.
unsafe fn set_socket_int(fd: i32, optname: libc::c_int, value: libc::c_int) -> std::io::Result<()> {
    let rc = unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            optname,
            &value as *const _ as *const libc::c_void,
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        )
    };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// SSDP packet building
// ---------------------------------------------------------------------------

/// Build a UPnP M-SEARCH packet. Mirrors the Go side exactly (including
/// the trailing blank line — `PlainAPP` sends three `\n`s, the spec
/// requires two; we preserve PlainAPP's behaviour for wire compatibility).
pub fn build_m_search(st: &str) -> String {
    let st = st.trim();
    let st_final = if st.is_empty() { "ssdp:all" } else { st };
    format!(
        "M-SEARCH * HTTP/1.1\n\
         HOST: 239.255.255.250:1900\n\
         MAN: \"ssdp:discover\"\n\
         MX: 3\n\
         ST: {st_final}\n\
         \n\
         \n"
    )
}

// ---------------------------------------------------------------------------
// Interface enumeration — see plain_rs::utils::ifaddr.
// ---------------------------------------------------------------------------

/// Enumerate the host's up, non-loopback IPv4 addresses and compute their
/// subnet-directed broadcast addresses. The first entry is the "auto" bind
/// (0.0.0.0) that the Go side prepends — it lets the kernel pick the
/// outgoing interface for multicast and helps on multi-homed hosts.
pub fn local_ipv4_addrs() -> std::collections::HashMap<Ipv4Addr, Option<Ipv4Addr>> {
    let mut out = std::collections::HashMap::new();
    for iface in crate::utils::ifaddr::list() {
        let ip = iface.ip;
        let bcast = iface
            .netmask
            .map(|mask| Ipv4Addr::from(u32::from(ip) | !u32::from(mask)));
        out.insert(ip, bcast);
    }
    out
}

/// Build the bind list the way the Go side does: prepend `0.0.0.0` then
/// dedupe by IP. The first entry MUST be the unspecified address so the
/// kernel can choose a route/interface for multicast.
pub fn bind_list() -> Vec<LocalIpv4Addr> {
    let mut out = vec![LocalIpv4Addr {
        ip: Ipv4Addr::UNSPECIFIED,
        broadcast: None,
    }];
    for (ip, bcast) in local_ipv4_addrs() {
        out.push(LocalIpv4Addr {
            ip,
            broadcast: bcast,
        });
    }
    out
}

/// Read extra destinations from `PLAINNAS_DLNA_SSDP_EXTRA_DESTS`. Each
/// entry is `IP[:port]` (port defaults to 1900).
pub fn ssdp_extra_destinations() -> Vec<SocketAddrV4> {
    let Ok(v) = std::env::var("PLAINNAS_DLNA_SSDP_EXTRA_DESTS") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for raw in v.split(',') {
        let p = raw.trim();
        if p.is_empty() {
            continue;
        }
        let addr_str = if p.contains(':') {
            p.to_string()
        } else {
            format!("{p}:1900")
        };
        if let Ok(sa) = addr_str.parse::<SocketAddrV4>() {
            if seen.insert(sa) {
                out.push(sa);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Send / receive
// ---------------------------------------------------------------------------

/// Send M-SEARCH packets on `socket` for every target × repeat. Caller
/// must have already called `set_deadline(deadline)`.
fn send_msearch_packets(
    socket: &UdpSocket,
    mcast: SocketAddrV4,
    bcast: Option<SocketAddrV4>,
    extras: &[SocketAddrV4],
    search_targets: &[String],
) {
    for st in search_targets {
        let q = build_m_search(st);
        if dlna_debug_payload_enabled() {
            log::debug!("ssdp: tx st={st}\n{}", truncate_for_log(&q, 1024));
        }
        for i in 0..M_SEARCH_REPEAT {
            let _ = socket.send_to(q.as_bytes(), mcast);
            if let Some(b) = bcast {
                let _ = socket.send_to(q.as_bytes(), b);
            }
            for ed in extras {
                let _ = socket.send_to(q.as_bytes(), *ed);
            }
            if dlna_debug_enabled() {
                log::info!("ssdp: send st={st} attempt={} mcast={mcast}", i + 1);
            }
            if i + 1 < M_SEARCH_REPEAT {
                std::thread::sleep(M_SEARCH_GAP);
            }
        }
    }
}

/// Read responses on `socket` until idle. Returns the de-duplicated list of
/// `UpnpDiscovered` records (keyed on `Location`).
fn read_responses(socket: &UdpSocket, deadline: std::time::Instant) -> Vec<UpnpDiscovered> {
    let mut out = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut buf = [0u8; RECV_BUF];
    let mut have_rx = false;
    let mut last_rx = std::time::Instant::now();

    loop {
        let now = std::time::Instant::now();
        if now >= deadline {
            break;
        }
        let chunk_dl = std::cmp::min(now + CHUNK_READ_DEADLINE, deadline);
        let dur = chunk_dl.duration_since(now);
        let fd = socket.as_raw_fd();
        // SAFETY: `fd` is owned by the socket (it isn't closed while
        // `socket` exists).
        unsafe {
            let _ = set_socket_timeout(fd, libc::SO_RCVTIMEO, Some(dur));
        }
        match socket.recv_from(&mut buf) {
            Ok((0, _)) => continue,
            Ok((n, sa)) => {
                have_rx = true;
                last_rx = std::time::Instant::now();
                let raw = &buf[..n];
                let resp = match std::str::from_utf8(raw) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let upper: String = resp
                    .chars()
                    .take(20)
                    .collect::<String>()
                    .trim()
                    .to_ascii_uppercase();
                if !(upper.starts_with("HTTP/1.1 200") || upper.starts_with("NOTIFY * HTTP")) {
                    continue;
                }
                let h = parse_ssdp_headers(resp);
                let loc = h
                    .get("location")
                    .cloned()
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if loc.is_empty() {
                    continue;
                }
                if !seen.insert(loc.clone()) {
                    continue;
                }
                let remote_ip = sa.ip().to_string();
                if dlna_debug_enabled() {
                    log::info!(
                        "ssdp: rx from={remote_ip} location={loc} usn={}",
                        h.get("usn").cloned().unwrap_or_default()
                    );
                }
                out.push(UpnpDiscovered {
                    location: loc,
                    usn: h.get("usn").cloned().unwrap_or_default().trim().to_string(),
                });
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::TimedOut
                    || e.kind() == std::io::ErrorKind::WouldBlock =>
            {
                if have_rx && last_rx.elapsed() >= IDLE_AFTER_LAST_RESPONSE {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    out
}

/// Run an SSDP search across all local IPv4 addresses for the given
/// targets, returning the de-duplicated `UpnpDiscovered` set.
pub fn ssdp_search(search_targets: &[String]) -> Vec<UpnpDiscovered> {
    if search_targets.is_empty() {
        return Vec::new();
    }
    let extras = ssdp_extra_destinations();
    let binds = bind_list();
    let deadline = std::time::Instant::now() + SSDP_HARD_DEADLINE;

    if dlna_debug_enabled() {
        log::info!("ssdp: starting search targets={search_targets:?} extras={extras:?}");
    }

    let mut combined: Vec<UpnpDiscovered> = Vec::new();
    let mut seen_loc: std::collections::HashSet<String> = std::collections::HashSet::new();

    for addr in &binds {
        let bind_ip = if addr.ip.is_unspecified() {
            Ipv4Addr::UNSPECIFIED
        } else {
            addr.ip
        };
        let bind_addr = SocketAddrV4::new(bind_ip, 0);
        let socket = match UdpSocket::bind(bind_addr) {
            Ok(s) => s,
            Err(e) => {
                log::info!("ssdp: bind failed ip={bind_ip} err={e}");
                continue;
            }
        };
        let fd = socket.as_raw_fd();
        // SAFETY: fd is the just-opened UDP socket and lives for the
        // duration of this loop iteration.
        unsafe {
            let _ = set_socket_int(fd, libc::SO_BROADCAST, 1);
            let _ = set_socket_int(fd, libc::SO_RCVBUF, KERNEL_BUF_BYTES);
            let _ = set_socket_int(fd, libc::SO_SNDBUF, KERNEL_BUF_BYTES);
            let _ = set_socket_timeout(fd, libc::SO_RCVTIMEO, Some(SSDP_HARD_DEADLINE));
            let _ = set_socket_timeout(fd, libc::SO_SNDTIMEO, Some(SSDP_HARD_DEADLINE));
        }

        let mcast = SocketAddrV4::new(SSDP_MCAST_ADDR, SSDP_MCAST_PORT);
        let bcast = addr
            .broadcast
            .map(|b| SocketAddrV4::new(b, SSDP_MCAST_PORT));
        send_msearch_packets(&socket, mcast, bcast, &extras, search_targets);
        for d in read_responses(&socket, deadline) {
            if seen_loc.insert(d.location.clone()) {
                combined.push(d);
            }
        }
    }

    if dlna_debug_enabled() {
        log::info!("ssdp: search complete count={}", combined.len());
    }
    combined
}

// ---------------------------------------------------------------------------
// SSDP header parsing
// ---------------------------------------------------------------------------

/// Parse the headers from a single SSDP response/notification text. The
/// value is `HashMap<lowercased-key, original-case-value>`. This is
/// used for both response messages and the same parsing feeds the
/// `upnp:renderer:found` event payload. Mirrors Go `parseSSDPPacket`.
pub fn parse_ssdp_headers(packet: &str) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    // Skip the request/response line. SSDP messages have a single
    // blank line separator; everything before it is the request line +
    // headers. We do case-insensitive header lookup, so we lowercase
    // the keys.
    let mut lines = packet.split('\n');
    let _ = lines.next(); // request line
    for line in lines {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            out.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    out
}

/// Parse the UDN out of a USN header, if present. Mirrors Go
/// `parseUDNFromUSN`. Example: `uuid:4d696e69-444c-164e-9d41-b827abcdef01::upnp:rootdevice`
/// → `uuid:4d696e69-444c-164e-9d41-b827abcdef01`.
pub fn parse_udn_from_usn(usn: &str) -> String {
    let s = usn.trim();
    let stripped = if let Some(rest) = s.strip_prefix("uuid:") {
        rest
    } else {
        s
    };
    let head = stripped.split("::").next().unwrap_or("");
    if head.is_empty() {
        return String::new();
    }
    if head.starts_with("uuid:") {
        head.to_string()
    } else {
        format!("uuid:{head}")
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/nas/dlna/ssdp.rs"]
mod tests;
