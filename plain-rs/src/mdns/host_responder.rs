//! Lightweight mDNS responder — single receive socket, standards-aware reply.
//!
//! Mirrors plain-app's Kotlin `MdnsHostResponder` (see `docs/mdns.md`): the
//! three bring-up resilience mechanisms are kept in lock-step with that code
//! so both platforms behave identically.
//!
//! RECEIVE: One socket bound to 0.0.0.0:5353 joins 224.0.0.251 on every valid
//! LAN interface. Network changes reuse the socket and only join missing
//! interfaces; transient create/bind/join failures retry with exponential
//! backoff.
//!
//! SEND: Replies are sent via the same socket so the source port is always
//! 5353. RFC 6762 §6.7 requires this — resolvers silently discard mDNS
//! responses whose source port ≠ 5353. QU/legacy-unicast queries are answered
//! directly; ordinary multicast queries are answered to 224.0.0.251:5353. The
//! full service info is broadcast when the socket comes up and re-broadcast
//! every 60s so neighbors whose caches expired still find us.

use super::packet_codec::{self, MdnsResponse, TYPE_A};
use super::service_info::{MdnsServiceInfo, PLAINAPP_SERVICE_TYPE};
use super::service_response_builder;
use if_addrs::{IfAddr, Interface};
use std::collections::HashSet;
use std::io;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};
use std::sync::{
    Arc, OnceLock, RwLock,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::Duration;

const MDNS_GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
const MDNS_GROUP_V6: Ipv6Addr = Ipv6Addr::new(0xff02, 0, 0, 0, 0, 0, 0, 0xfb);
const MDNS_PORT: u16 = 5353;
const RECEIVE_TIMEOUT_MS: u64 = 1_000;
const RECV_BUF_SIZE: usize = 1500;

/// How often to re-broadcast the service so neighbors whose cache expired
/// (multicast responses get lost) still find us: every 60s, < the 120s TTL.
const REANNOUNCE_MS: u64 = 60_000;

/// Exponential backoff for transient bring-up failures (create/bind/join).
const INITIAL_RETRY_MS: u64 = 2_000;
const MAX_RETRY_MS: u64 = 32_000;

pub type PacketListener = Arc<dyn Fn(&[u8], &str) + Send + Sync>;

struct Inner {
    hostname: RwLock<String>,
    service_info: RwLock<Option<MdnsServiceInfo>>,
    socket: RwLock<Option<Arc<std::net::UdpSocket>>>,
    running: AtomicBool,
    listeners: RwLock<Vec<PacketListener>>,
}

static INNER: Inner = Inner {
    hostname: RwLock::new(String::new()),
    service_info: RwLock::new(None),
    socket: RwLock::new(None),
    running: AtomicBool::new(false),
    listeners: RwLock::new(Vec::new()),
};

/// Whether any external (non-local) packet reached the shared multicast
/// socket since the last `take_external_multicast_seen` call.
static SAW_EXTERNAL_MULTICAST: AtomicBool = AtomicBool::new(false);

/// Dedicated QU query socket (ephemeral port), lazily created.
static QU_SOCKET: RwLock<Option<Arc<std::net::UdpSocket>>> = RwLock::new(None);

/// IPv6 mDNS receive socket: bound to [::]:5353, joined to `ff02::fb` so the
/// browser hears devices that announce over IPv6 link-local multicast when
/// they do not answer IPv4 multicast (e.g. behind an IPv4-inert LAN). Receive
/// and query-only; answers still go out the IPv4 socket on the shared port.
static IPV6_SOCKET: RwLock<Option<Arc<std::net::UdpSocket>>> = RwLock::new(None);
static IPV6_RUNNING: AtomicBool = AtomicBool::new(false);

/// Interface names the current socket has joined the group on (`None` = the
/// socket was torn down / nothing joined yet).
fn joined_ifaces() -> &'static std::sync::Mutex<Option<HashSet<String>>> {
    static JOINED: OnceLock<std::sync::Mutex<Option<HashSet<String>>>> = OnceLock::new();
    JOINED.get_or_init(|| std::sync::Mutex::new(None))
}

/// Delay for the next retry attempt; advanced on each failure, reset on start.
static RETRY_DELAY_MS: AtomicU64 = AtomicU64::new(INITIAL_RETRY_MS);
/// True while a retry attempt is scheduled (prevents stacking retry threads).
static RETRY_PENDING: AtomicBool = AtomicBool::new(false);
/// Marshals the single periodic re-announce thread.
static REANNOUNCE_STARTED: AtomicBool = AtomicBool::new(false);

/// Takes (reads and resets) the external-multicast-seen flag. The browser
/// polls this every scan cycle: no external multicast means the receive path
/// is dead (e.g. a router dropping cross-band multicast) and QU queries must
/// take over.
pub fn take_external_multicast_seen() -> bool {
    SAW_EXTERNAL_MULTICAST.swap(false, Ordering::Relaxed)
}

pub fn is_running() -> bool {
    INNER.running.load(Ordering::SeqCst) && INNER.socket.read().unwrap().is_some()
}

/// Starts the mDNS responder. `service` advertises the PlainApp service
/// (PTR/SRV/TXT/A answers); when None the responder only answers A-record
/// queries for `mdns_hostname`.
pub fn start(mdns_hostname: &str, service: Option<MdnsServiceInfo>) -> bool {
    let normalized = normalize_hostname(mdns_hostname);
    if normalized.is_empty() {
        log::error!("mDNS start skipped: empty hostname");
        return false;
    }
    *INNER.hostname.write().unwrap() = normalized;
    *INNER.service_info.write().unwrap() = service;
    RETRY_DELAY_MS.store(INITIAL_RETRY_MS, Ordering::SeqCst);
    let ok = restart_socket();
    ensure_ipv6_started();
    ok
}

/// Ensures the responder socket is up so discovery works even while the HTTP
/// service is off. When already running this keeps the current configuration.
pub fn ensure_started(mdns_hostname: &str) -> bool {
    if is_running() {
        return true;
    }
    let service = INNER.service_info.read().unwrap().clone();
    start(mdns_hostname, service)
}

/// Replaces the published service and re-announces it right away instead of
/// waiting for the next REANNOUNCE_MS cycle. Used when the advertised data
/// changed while the service stays up (e.g. the device was renamed).
///
/// A different instance FQDN makes the old instance's records stale in every
/// peer cache, so the previous instance is withdrawn first with an RFC 6762
/// §8.4 goodbye (TTL=0); peers then list only the new name.
///
/// No-op when nothing is published: with the HTTP service off the responder
/// only answers hostname queries, and republishing would advertise a dead
/// service. Returns false when the responder socket is down — the new service
/// is stored either way and goes out with the next
/// `start`/`restart_socket`. Mirrors the Kotlin `MdnsHostResponder.updateService`.
pub fn update_service(service: MdnsServiceInfo) -> bool {
    let fqdn = service.instance_fqdn();
    let previous = {
        let mut guard = INNER.service_info.write().unwrap();
        match guard.clone() {
            None => return false,
            Some(prev) => {
                *guard = Some(service);
                prev
            }
        }
    };
    if !is_running() {
        log::info!("mDNS updateService: responder down, stored {fqdn}");
        return false;
    }
    if !previous.instance_fqdn().eq_ignore_ascii_case(&fqdn) {
        send_goodbye(&previous);
    }
    broadcast_service();
    true
}

/// Sends the TTL=0 goodbye for `previous` on every LAN interface. Mirrors the
/// Kotlin `MdnsHostResponder.sendGoodbye`.
fn send_goodbye(previous: &MdnsServiceInfo) {
    let Some(socket) = INNER.socket.read().unwrap().clone() else {
        return;
    };
    let candidates = candidate_interfaces();
    if candidates.is_empty() {
        return;
    }
    let bytes = service_response_builder::build_goodbye(previous);
    let target = SocketAddrV4::new(MDNS_GROUP, MDNS_PORT);
    log::info!(
        "mDNS goodbye {} -> {MDNS_GROUP}:{MDNS_PORT} on {} iface(s)",
        previous.instance_fqdn(),
        candidates.len()
    );
    for iface in &candidates {
        let _ = socket2::SockRef::from(&*socket).set_multicast_if_v4(&iface.ip);
        if let Err(e) = socket.send_to(&bytes, target) {
            log::error!("mDNS goodbye {}: {e}", iface.name);
        }
    }
}

/// Brings the responder up for the current network. Reuses the already-bound
/// socket when one exists, only joining interfaces that are missing — so a
/// network change does not tear down and rebuild (no dropped membership, no
/// churn). The socket is rebuilt only when it does not exist yet. Mirrors the
/// Kotlin `MdnsHostResponder.restartSocket`.
pub fn restart_socket() -> bool {
    let hostname = INNER.hostname.read().unwrap().clone();
    if hostname.is_empty() {
        log::error!("mDNS restart skipped: hostname not configured");
        return false;
    }

    let candidates = candidate_interfaces();
    if candidates.is_empty() {
        log::error!("mDNS: no candidate interfaces found");
        return false;
    }

    // Reuse the live socket when present so a network change does not rebuild
    // it (no dropped membership, no churn); rebuild when missing or the worker
    // died (mirrors Kotlin's `existing.isClosed`).
    let existing = INNER.socket.read().unwrap().clone();
    let is_new = !(existing.is_some() && INNER.running.load(Ordering::SeqCst));
    let socket = if !is_new {
        existing.expect("checked alive")
    } else {
        match create_and_bind_socket() {
            Ok(s) => {
                let socket = Arc::new(s);
                if let Some(old) = INNER.socket.write().unwrap().replace(socket.clone()) {
                    let _ = old.leave_multicast_v4(&MDNS_GROUP, &Ipv4Addr::UNSPECIFIED);
                }
                INNER.running.store(true, Ordering::SeqCst);
                let worker = Worker {
                    socket: socket.clone(),
                };
                std::thread::Builder::new()
                    .name("plain-mdns-responder".to_string())
                    .spawn(move || worker.run_loop())
                    .expect("spawn mdns responder");
                *joined_ifaces().lock().unwrap() = None;
                socket
            }
            Err(e) => {
                log::error!("mDNS create/bind failed: {e}");
                schedule_retry();
                return false;
            }
        }
    };

    let ok = sync_memberships(&socket, &candidates);
    if !ok && is_new {
        schedule_retry();
    }
    log::info!(
        "mDNS listener up hostname={hostname} interfaces={:?}",
        joined_iface_list()
    );
    broadcast_service();
    ensure_reannounce();
    ok
}

/// Reuses the socket and joins the group only on interfaces that are missing,
/// keeping existing memberships so the socket is never rebuilt on an
/// interface-only change. Falls back to a plain (interface-less) join only
/// when no per-interface join works. Mirrors Kotlin `syncMemberships`.
fn sync_memberships(socket: &std::net::UdpSocket, candidates: &[MdnsIface]) -> bool {
    let desired: HashSet<String> = candidates.iter().map(|i| i.name.clone()).collect();
    let joined = joined_iface_set();
    let to_join = interfaces_to_join(&desired, &joined);
    let mut fresh = HashSet::new();
    for name in to_join {
        if let Some(iface) = candidates.iter().find(|i| i.name == name) {
            match socket.join_multicast_v4(&MDNS_GROUP, &iface.ip) {
                Ok(()) => {
                    fresh.insert(name.clone());
                    log::debug!("mDNS joined {}", name);
                }
                Err(e) => log::error!("mDNS joinGroup {name}: {e}"),
            }
        }
    }
    let success: HashSet<String> = desired
        .intersection(&joined)
        .cloned()
        .chain(fresh)
        .collect();
    *joined_ifaces().lock().unwrap() = Some(success.clone());
    if success.is_empty() {
        // All per-interface joins failed (EINVAL on some kernels) — fall back
        // to the default interface so single-NIC devices still work.
        match socket.join_multicast_v4(&MDNS_GROUP, &Ipv4Addr::UNSPECIFIED) {
            Ok(()) => log::debug!("mDNS joined (default)"),
            Err(e) => log::error!("mDNS joinGroup default: {e}"),
        }
    }
    !success.is_empty()
}

/// Interface names that are desired but not yet joined — pure, unit-tested.
/// Mirrors Kotlin `MdnsHostResponder.interfacesToJoin`.
fn interfaces_to_join(desired: &HashSet<String>, joined: &HashSet<String>) -> HashSet<String> {
    desired.difference(joined).cloned().collect()
}

/// Next backoff delay after an unsuccessful attempt — pure, unit-tested.
/// Mirrors Kotlin `MdnsHostResponder.nextRetryDelay`.
fn next_retry_delay(current: u64) -> u64 {
    current.saturating_mul(2).min(MAX_RETRY_MS)
}

/// Reschedules a bring-up attempt with exponential backoff (2s → 4s → … → 32s).
/// At most one retry is pending at a time. Because [restart_socket] reuses a
/// live socket, a stale retry that fires after a successful bring-up is
/// idempotent (it just re-joins and re-announces), so no cancellation is needed.
fn schedule_retry() {
    if RETRY_PENDING.swap(true, Ordering::SeqCst) {
        return;
    }
    let delay = RETRY_DELAY_MS.load(Ordering::SeqCst);
    std::thread::Builder::new()
        .name("plain-mdns-retry".to_string())
        .spawn(move || {
            std::thread::sleep(Duration::from_millis(delay));
            RETRY_PENDING.store(false, Ordering::SeqCst);
            RETRY_DELAY_MS.store(next_retry_delay(delay), Ordering::SeqCst);
            restart_socket();
        })
        .ok();
}

/// Starts the periodic re-announcer (singleton). Broadcasts the full service
/// info every 60s so neighbors whose cache expired (multicast responses are
/// lossy) still discover us without waiting for their own query. Mirrors the
/// Kotlin `ensureReannounceJob`.
fn ensure_reannounce() {
    if REANNOUNCE_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::Builder::new()
        .name("plain-mdns-announce".to_string())
        .spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(REANNOUNCE_MS));
                if INNER.service_info.read().unwrap().is_some() {
                    broadcast_service();
                }
            }
        })
        .ok();
}

/// Sends a gratuitous mDNS announcement (RFC 6762 §8.3): the full service info
/// when a service is published, else the hostname A record. Each copy carries
/// ONLY its outgoing interface's address — announcing every interface's IP in
/// one packet makes peers whose browsers replace the A-record set per record
/// latch onto the wrong (e.g. VPN) address. Mirrors the Kotlin
/// `MdnsHostResponder.broadcastService` + `buildAnnouncement`.
fn broadcast_service() {
    let Some(socket) = INNER.socket.read().unwrap().clone() else {
        return;
    };
    let candidates = candidate_interfaces();
    if candidates.is_empty() {
        return;
    }
    let service = INNER.service_info.read().unwrap().clone();
    let hostname = INNER.hostname.read().unwrap().clone();

    let target = SocketAddrV4::new(MDNS_GROUP, MDNS_PORT);
    log::info!(
        "mDNS announce -> {MDNS_GROUP}:{MDNS_PORT} on {} iface(s)",
        candidates.len()
    );
    for iface in &candidates {
        let ip = iface.ip.to_string();
        let bytes: Option<Vec<u8>> = match service.clone() {
            Some(mut s) => {
                s.ips = vec![ip];
                service_response_builder::build_response_if_match(
                    &packet_codec::build_ptr_query(PLAINAPP_SERVICE_TYPE),
                    &s,
                )
                .map(|r| r.bytes)
            }
            None => packet_codec::build_response_if_match(
                &packet_codec::build_query(&hostname, TYPE_A, false),
                &hostname,
                &[ip],
            )
            .map(|r| r.bytes),
        };
        let Some(bytes) = bytes else {
            continue;
        };
        let _ = socket2::SockRef::from(&*socket).set_multicast_if_v4(&iface.ip);
        if let Err(e) = socket.send_to(&bytes, target) {
            log::error!("mDNS announce {}: {e}", iface.name);
        }
    }
}

/// Registers a listener for every inbound mDNS packet; survives socket restarts.
pub fn add_packet_listener(listener: PacketListener) {
    let mut listeners = INNER.listeners.write().unwrap();
    if !listeners.iter().any(|l| Arc::ptr_eq(l, &listener)) {
        listeners.push(listener);
    }
}

/// Sends an mDNS query through the shared socket so responses come back on
/// port 5353 (RFC 6762 §6.7 requires the source port to be 5353).
pub fn send_query(bytes: &[u8]) {
    if let Some(socket) = INNER.socket.read().unwrap().clone() {
        send_to_group(&socket, bytes);
    }
    send_ipv6_query(bytes);
}

/// Sends a query directly to a known peer's `ip:5353` (RFC 6762 §5.5 direct
/// unicast requery) — the discovery fallback for networks that silently drop
/// multicast (VPN / AP isolation) while unicast still flows. The QU bit in the
/// packet asks the peer to answer via unicast, which arrives on the shared
/// receive loops like any multicast reply.
pub fn send_unicast_query(bytes: &[u8], ip: &str) {
    if let Ok(v4) = ip.parse::<Ipv4Addr>() {
        // Through the ephemeral QU socket: a unicast reply addressed to
        // :5353 could be delivered to another SO_REUSEPORT peer on this
        // machine (e.g. the system mDNSResponder), while a reply to the
        // ephemeral source port always comes back to this exact socket
        // (RFC 6762 §6.7 legacy-unicast path).
        if let Some(socket) = ensure_qu_socket() {
            if let Err(e) = socket.send_to(bytes, SocketAddrV4::new(v4, MDNS_PORT)) {
                log::error!("mDNS unicast query to {ip}: {e}");
            }
        }
        return;
    }
    if let Ok(v6) = ip.parse::<Ipv6Addr>() {
        let Some(socket) = IPV6_SOCKET.read().unwrap().clone() else {
            return;
        };
        if let Err(e) = socket.send_to(bytes, SocketAddrV6::new(v6, MDNS_PORT, 0, 0)) {
            log::error!("mDNS unicast query to {ip}: {e}");
        }
    }
}

/// Sends a QU (unicast-response requested, RFC 6762 §5.4) query through a
/// dedicated ephemeral-port socket. Broken routers drop cross-band multicast
/// while unicast still flows; the unicast responses then come back to this
/// exact socket — a 5353-bound socket could lose them to another
/// SO_REUSEPORT peer on the same machine.
pub fn send_qu_query(bytes: &[u8]) {
    if let Some(socket) = ensure_qu_socket() {
        send_to_group(&socket, bytes);
    }
    send_ipv6_query(bytes);
}

fn send_to_group(socket: &std::net::UdpSocket, bytes: &[u8]) {
    let target = SocketAddrV4::new(MDNS_GROUP, MDNS_PORT);
    let candidates = candidate_interfaces();
    if candidates.is_empty() {
        if let Err(e) = socket.send_to(bytes, target) {
            log::error!("mDNS sendQuery: {e}");
        }
        return;
    }
    // Send once per interface: the multicast egress interface is a socket-wide
    // setting, so a single send can only leave one NIC. Picking just the first
    // candidate silently drops the query when that interface is not where the
    // peers live (e.g. Ethernet/VM bridge enumerated before Wi-Fi).
    for iface in candidates {
        let _ = socket2::SockRef::from(socket).set_multicast_if_v4(&iface.ip);
        if let Err(e) = socket.send_to(bytes, target) {
            log::error!("mDNS sendQuery {}: {e}", iface.name);
        }
    }
}

fn ipv6_joined() -> &'static std::sync::Mutex<Option<HashSet<u32>>> {
    static JOINED: OnceLock<std::sync::Mutex<Option<HashSet<u32>>>> = OnceLock::new();
    JOINED.get_or_init(|| std::sync::Mutex::new(None))
}

/// IPv6-capable multicast interfaces (name, interface index). Excludes loopback
/// and addresses with no usable scope id.
pub fn candidate_ipv6_interfaces() -> Vec<(String, u32)> {
    let mut seen = HashSet::new();
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|iface| match iface.addr {
            IfAddr::V6(v6) => {
                let index = iface.index?;
                if v6.ip.is_loopback() || v6.ip.is_multicast() || index == 0 {
                    return None;
                }
                if !seen.insert(index) {
                    return None;
                }
                Some((iface.name, index))
            }
            _ => None,
        })
        .collect()
}

/// This host's own non-loopback IPv6 addresses, used to ignore the multicast
/// loop-back of our own IPv6 queries.
pub fn candidate_ipv6_addresses() -> Vec<String> {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|iface| match iface.addr {
            IfAddr::V6(v6) if !v6.ip.is_loopback() => Some(v6.ip.to_string()),
            _ => None,
        })
        .collect()
}

pub fn is_local_ipv6(ip: &str) -> bool {
    candidate_ipv6_addresses().iter().any(|a| a == ip)
}

fn create_ipv6_socket() -> io::Result<std::net::UdpSocket> {
    let socket = socket2::Socket::new(
        socket2::Domain::IPV6,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    socket.set_multicast_loop_v6(true)?;
    socket.set_only_v6(true)?;
    socket.bind(&SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, MDNS_PORT, 0, 0).into())?;
    Ok(socket.into())
}

/// Joins `ff02::fb` on IPv6 interfaces that are missing it, reusing the socket
/// so a network change only adds memberships. Best-effort: failure never tears
/// down the IPv4 path.
fn sync_ipv6_memberships(socket: &std::net::UdpSocket) -> bool {
    let desired: Vec<u32> = candidate_ipv6_interfaces()
        .iter()
        .map(|(_, i)| *i)
        .collect();
    let joined = ipv6_joined().lock().unwrap().clone().unwrap_or_default();
    let mut fresh = HashSet::new();
    for index in &desired {
        if joined.contains(index) {
            continue;
        }
        match socket2::SockRef::from(socket).join_multicast_v6(&MDNS_GROUP_V6, *index) {
            Ok(()) => {
                fresh.insert(*index);
                log::debug!("mDNS joined v6 iface {index}");
            }
            Err(e) => log::debug!("mDNS joinGroup v6 {index}: {e}"),
        }
    }
    let success: HashSet<u32> = desired
        .iter()
        .filter(|i| joined.contains(i))
        .copied()
        .chain(fresh)
        .collect();
    *ipv6_joined().lock().unwrap() = Some(success.clone());
    !success.is_empty() || desired.is_empty()
}

/// Brings the IPv6 `ff02::fb` listener up so IPv6 multicast announcements are
/// received even on networks where devices never answer IPv4 multicast.
pub fn ensure_ipv6_started() -> bool {
    let existing = IPV6_SOCKET.read().unwrap().clone();
    if !(IPV6_RUNNING.load(Ordering::SeqCst) && existing.is_some()) {
        match create_ipv6_socket() {
            Ok(s) => {
                let socket = Arc::new(s);
                *IPV6_SOCKET.write().unwrap() = Some(socket.clone());
                IPV6_RUNNING.store(true, Ordering::SeqCst);
                *ipv6_joined().lock().unwrap() = None;
                let worker = socket.clone();
                std::thread::Builder::new()
                    .name("plain-mdns-ipv6".to_string())
                    .spawn(move || run_ipv6_loop(&worker))
                    .expect("spawn mdns ipv6");
            }
            Err(e) => {
                log::error!("mDNS IPv6 create/bind failed: {e}");
                return false;
            }
        }
    }
    if let Some(socket) = IPV6_SOCKET.read().unwrap().clone() {
        return sync_ipv6_memberships(&socket);
    }
    false
}

fn run_ipv6_loop(socket: &Arc<std::net::UdpSocket>) {
    let _ = socket.set_read_timeout(Some(Duration::from_millis(RECEIVE_TIMEOUT_MS)));
    let mut buf = [0u8; RECV_BUF_SIZE];
    loop {
        match socket.recv_from(&mut buf) {
            Ok((n, src)) => {
                let std::net::IpAddr::V6(v6) = src.ip() else {
                    continue;
                };
                let sender = v6.to_string();
                if is_local_ipv6(&sender) {
                    continue;
                }
                SAW_EXTERNAL_MULTICAST.store(true, Ordering::Relaxed);
                notify_packet_listeners(&buf[..n], &sender);
            }
            Err(err)
                if err.kind() == io::ErrorKind::WouldBlock
                    || err.kind() == io::ErrorKind::TimedOut => {}
            Err(err) => {
                log::debug!("mDNS IPv6 receive error, stopping: {err}");
                if still_current_ipv6(&socket) {
                    IPV6_RUNNING.store(false, Ordering::SeqCst);
                }
                break;
            }
        }
        if !IPV6_RUNNING.load(Ordering::SeqCst) {
            break;
        }
    }
}

fn still_current_ipv6(socket: &Arc<std::net::UdpSocket>) -> bool {
    matches!(
        IPV6_SOCKET.read().unwrap().as_ref(),
        Some(cur) if Arc::ptr_eq(socket, cur)
    )
}

/// Sends a query to the IPv6 multicast group on every IPv6 interface, so
/// devices that only answer IPv6 multicast are still discovered. The DNS
/// payload is address-family independent — the v4 packet bytes work as-is.
fn send_ipv6_query(bytes: &[u8]) {
    let Some(socket) = IPV6_SOCKET.read().unwrap().clone() else {
        return;
    };
    let target = SocketAddrV6::new(MDNS_GROUP_V6, MDNS_PORT, 0, 0);
    let interfaces = candidate_ipv6_interfaces();
    if interfaces.is_empty() {
        let _ = socket.send_to(bytes, target);
        return;
    }
    for (name, index) in interfaces {
        let _ = socket2::SockRef::from(&*socket).set_multicast_if_v6(index);
        if let Err(e) = socket.send_to(bytes, target) {
            log::error!("mDNS sendQuery v6 {}: {e}", name);
        }
    }
}

fn ensure_qu_socket() -> Option<Arc<std::net::UdpSocket>> {
    if let Some(socket) = QU_SOCKET.read().unwrap().clone() {
        return Some(socket);
    }
    let mut guard = QU_SOCKET.write().unwrap();
    if let Some(socket) = guard.clone() {
        return Some(socket);
    }
    let socket = match create_qu_socket() {
        Ok(s) => Arc::new(s),
        Err(e) => {
            log::error!("mDNS QU socket create failed: {e}");
            return None;
        }
    };
    *guard = Some(socket.clone());
    let reader = socket.clone();
    std::thread::Builder::new()
        .name("plain-mdns-qu-reader".to_string())
        .spawn(move || qu_receive_loop(&reader))
        .expect("spawn mdns qu reader");
    Some(socket)
}

fn qu_receive_loop(socket: &std::net::UdpSocket) {
    let mut buf = [0u8; RECV_BUF_SIZE];
    loop {
        match socket.recv_from(&mut buf) {
            Ok((n, src)) => {
                if let std::net::IpAddr::V4(v4) = src.ip() {
                    notify_packet_listeners(&buf[..n], &v4.to_string());
                }
            }
            Err(err) => {
                log::debug!("mDNS QU receive error, stopping: {err}");
                break;
            }
        }
    }
}

fn create_qu_socket() -> io::Result<std::net::UdpSocket> {
    let socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )?;
    socket.set_multicast_ttl_v4(1)?;
    socket.set_multicast_loop_v4(false)?;
    socket.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into())?;
    Ok(socket.into())
}

fn notify_packet_listeners(bytes: &[u8], sender_ip: &str) {
    let listeners = INNER.listeners.read().unwrap().clone();
    for l in listeners {
        l(bytes, sender_ip);
    }
}

struct Worker {
    socket: Arc<std::net::UdpSocket>,
}

impl Worker {
    fn run_loop(&self) {
        let _ = self
            .socket
            .set_read_timeout(Some(Duration::from_millis(RECEIVE_TIMEOUT_MS)));
        let mut buf = [0u8; RECV_BUF_SIZE];
        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((n, src)) => {
                    let sender_ip = match src.ip() {
                        std::net::IpAddr::V4(v4) => v4.to_string(),
                        std::net::IpAddr::V6(_) => continue,
                    };
                    let packet = buf[..n].to_vec();
                    let local = is_local_ip(&sender_ip);
                    // Any external packet proves the multicast receive path
                    // works; the browser polls this for QU fallback.
                    if !local {
                        SAW_EXTERNAL_MULTICAST.store(true, Ordering::Relaxed);
                    }
                    notify_packet_listeners(&packet, &sender_ip);
                    // Our own multicast packets loop back to this socket;
                    // answering them doubles traffic on every discovery cycle.
                    if local {
                        continue;
                    }
                    let fresh = candidate_interfaces();
                    if fresh.is_empty() {
                        continue;
                    }
                    let Some((response_iface, local_ip)) = find_response_iface(&sender_ip, &fresh)
                    else {
                        continue;
                    };
                    let Some(response) = build_response(&packet, &[local_ip.clone()]) else {
                        continue;
                    };
                    let use_unicast =
                        response.unicast_response_requested() || src.port() != MDNS_PORT;
                    let dest = if use_unicast {
                        src
                    } else {
                        std::net::SocketAddr::V4(SocketAddrV4::new(MDNS_GROUP, MDNS_PORT))
                    };
                    let send_result = (|| -> io::Result<()> {
                        if !use_unicast {
                            socket2::SockRef::from(&*self.socket)
                                .set_multicast_if_v4(&response_iface.ip)?;
                        }
                        self.socket.send_to(&response.bytes, dest)?;
                        Ok(())
                    })();
                    if let Err(e) = send_result {
                        log::error!("mDNS send to {sender_ip}: {e}");
                    }
                }
                Err(err)
                    if err.kind() == io::ErrorKind::WouldBlock
                        || err.kind() == io::ErrorKind::TimedOut => {}
                Err(err) => {
                    // A non-timeout error means this worker's socket is dead.
                    // Clear running ONLY if this worker still owns the current
                    // socket — a stale worker from a rebuild must not flip the
                    // flag for the newer one. This lets a future restart_socket
                    // detect the dead socket (mirrors Kotlin's `isClosed`).
                    log::debug!("mDNS receive error, stopping worker: {err}");
                    let still_current = matches!(
                        INNER.socket.read().unwrap().as_ref(),
                        Some(cur) if Arc::ptr_eq(&self.socket, cur)
                    );
                    if still_current {
                        INNER.running.store(false, Ordering::SeqCst);
                    }
                    break;
                }
            }
            if !INNER.running.load(Ordering::SeqCst) {
                break;
            }
        }
    }
}

/// Answers a query with the PlainApp service records when one is published,
/// otherwise falls back to the A-record hostname responder.
fn build_response(query: &[u8], ips: &[String]) -> Option<MdnsResponse> {
    let service = INNER.service_info.read().unwrap().clone();
    if let Some(mut service) = service {
        service.ips = ips.to_vec();
        let service_response = service_response_builder::build_response_if_match(query, &service)?;
        let matched_questions = packet_codec::read_questions(query)?;
        return Some(MdnsResponse {
            bytes: service_response.bytes,
            matched_questions,
        });
    }
    let hostname = INNER.hostname.read().unwrap().clone();
    packet_codec::build_response_if_match(query, &hostname, ips)
}

pub fn normalize_hostname(value: &str) -> String {
    let trimmed = value.trim().trim_matches('.').to_lowercase();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.ends_with(".local") {
        trimmed
    } else {
        format!("{trimmed}.local")
    }
}

fn create_and_bind_socket() -> io::Result<std::net::UdpSocket> {
    let socket = socket2::Socket::new(
        socket2::Domain::IPV4,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    socket.set_multicast_ttl_v4(1)?;
    socket.set_multicast_loop_v4(true)?;
    socket.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, MDNS_PORT).into())?;
    Ok(socket.into())
}

/// LAN interfaces with their IPv4 address, used for group join + subnet match.
pub struct MdnsIface {
    pub name: String,
    pub ip: Ipv4Addr,
    pub netmask: Ipv4Addr,
}

pub fn candidate_interfaces() -> Vec<MdnsIface> {
    let interfaces: Vec<Interface> = if_addrs::get_if_addrs().unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    interfaces
        .into_iter()
        .filter_map(|iface| match iface.addr {
            IfAddr::V4(v4) => {
                let ip = v4.ip;
                if ip.is_loopback() || ip.is_link_local() || ip.is_unspecified() {
                    return None;
                }
                if !seen.insert(ip) {
                    return None;
                }
                Some(MdnsIface {
                    name: iface.name,
                    ip,
                    netmask: v4.netmask,
                })
            }
            _ => None,
        })
        .collect()
}

fn find_response_iface(sender_ip: &str, candidates: &[MdnsIface]) -> Option<(MdnsIface, String)> {
    let sender: Ipv4Addr = sender_ip.parse().ok()?;
    for iface in candidates {
        let a = u32::from(iface.ip) & u32::from(iface.netmask);
        let b = u32::from(sender) & u32::from(iface.netmask);
        if a == b {
            return Some((
                MdnsIface {
                    name: iface.name.clone(),
                    ip: iface.ip,
                    netmask: iface.netmask,
                },
                iface.ip.to_string(),
            ));
        }
    }
    None
}

pub fn local_ipv4_strs() -> Vec<String> {
    candidate_interfaces()
        .into_iter()
        .map(|iface| iface.ip.to_string())
        .collect()
}

/// Whether `ip` is one of this host's own IPv4 addresses. Used to ignore
/// multicast loop-back of our own queries/announcements (RFC 6762 §5.2).
pub fn is_local_ip(ip: &str) -> bool {
    candidate_interfaces()
        .into_iter()
        .any(|iface| iface.ip.to_string() == ip)
}

/// Picks the first candidate IP that shares a subnet with a local interface,
/// falling back to the first entry.
pub fn get_best_ip(ips: &[String]) -> String {
    match ips.first() {
        None => String::new(),
        Some(first) if ips.len() == 1 => first.clone(),
        Some(first) => {
            let locals = candidate_interfaces();
            for ip in ips {
                if find_response_iface(ip, &locals).is_some() {
                    return ip.clone();
                }
            }
            first.clone()
        }
    }
}

fn joined_iface_set() -> HashSet<String> {
    joined_ifaces().lock().unwrap().clone().unwrap_or_default()
}

fn joined_iface_list() -> Vec<String> {
    let mut names: Vec<String> = joined_iface_set().into_iter().collect();
    names.sort();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(from: &[&str]) -> HashSet<String> {
        from.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn normalize_hostname_appends_local() {
        assert_eq!(normalize_hostname("Ab"), "ab.local");
        assert_eq!(normalize_hostname("ab.local"), "ab.local");
        assert_eq!(normalize_hostname(" AB.LOCAL. "), "ab.local");
        assert_eq!(normalize_hostname(""), "");
    }

    #[test]
    fn candidate_interfaces_exclude_loopback() {
        for iface in candidate_interfaces() {
            assert!(!iface.ip.is_loopback());
        }
    }

    #[test]
    fn find_response_iface_matches_subnet() {
        let candidates = vec![MdnsIface {
            name: "en0".to_string(),
            ip: "192.168.1.5".parse().unwrap(),
            netmask: "255.255.255.0".parse().unwrap(),
        }];
        let (iface, ip) = find_response_iface("192.168.1.100", &candidates).expect("same subnet");
        assert_eq!(iface.name, "en0");
        assert_eq!(ip, "192.168.1.5");
        assert!(find_response_iface("10.0.0.1", &candidates).is_none());
    }

    #[test]
    fn is_local_ip_recognizes_candidate_addresses() {
        let locals = local_ipv4_strs();
        for ip in &locals {
            assert!(is_local_ip(ip), "{ip} should be local");
        }
        // A fabricated external address is never local.
        assert!(!is_local_ip("192.0.2.1"));
        assert!(!is_local_ip(""));
    }

    // ── interfaces_to_join (incremental multicast membership) ──────────────
    #[test]
    fn nothing_joined_yet_all_desired_interfaces_need_joining() {
        let got = interfaces_to_join(&set(&["wlan0", "ap0"]), &set(&[]));
        assert_eq!(got, set(&["wlan0", "ap0"]));
    }

    #[test]
    fn partially_joined_only_the_missing_ones_are_returned() {
        let got = interfaces_to_join(&set(&["wlan0", "ap0"]), &set(&["wlan0"]));
        assert_eq!(got, set(&["ap0"]));
    }

    #[test]
    fn fully_joined_nothing_to_join() {
        let got = interfaces_to_join(&set(&["wlan0", "ap0"]), &set(&["wlan0", "ap0"]));
        assert!(got.is_empty());
    }

    #[test]
    fn stale_joined_interfaces_not_in_desired_are_ignored() {
        let got = interfaces_to_join(&set(&["wlan0"]), &set(&["tun0"]));
        assert_eq!(got, set(&["wlan0"]));
    }

    // ── next_retry_delay (exponential backoff) ────────────────────────────
    #[test]
    fn backoff_doubles() {
        assert_eq!(next_retry_delay(2_000), 4_000);
    }

    #[test]
    fn backoff_doubles_up_to_the_cap() {
        assert_eq!(next_retry_delay(8_000), 16_000);
        assert_eq!(next_retry_delay(16_000), 32_000);
    }

    #[test]
    fn backoff_never_exceeds_the_cap() {
        assert_eq!(next_retry_delay(32_000), 32_000);
        assert_eq!(next_retry_delay(std::u64::MAX), 32_000);
    }
}
