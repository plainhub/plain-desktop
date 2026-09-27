//! mDNS service browser for `_plainapp._tcp.local`.
//!
//! Flow (RFC 6762 browsing):
//!  1. periodically send a PTR query for the service type
//!  2. parse PTR responses to learn instance names
//!  3. for each new instance send SRV + TXT (+ A) queries
//!  4. combine port / metadata / IPs into a `FoundDevice`
//!
//! The browser shares the host responder's socket (one bind on 5353), so its
//! queries and the responder's answers stay on the same port.

use super::host_responder;
use super::packet_codec::{self, TYPE_A, TYPE_AAAA, TYPE_PTR, TYPE_SRV, TYPE_TXT};
use super::service_info::PLAINAPP_SERVICE_TYPE;
use std::collections::{HashMap, HashSet};
use std::sync::{
    Arc, Mutex, RwLock,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DISCOVER_INTERVAL_MS: u64 = 5_000;
/// Re-query an incomplete instance at most this often (multicast responses
/// get lost).
const FOLLOW_UP_RETRY_MS: u64 = 10_000;
/// Switch to QU (unicast-response) queries after this many scan cycles with
/// zero external multicast packets — broken routers silently drop cross-band
/// multicast while unicast still flows.
const QU_FALLBACK_AFTER_CYCLES: u64 = 2;

/// A complete service instance handed to the discovery consumer. TXT values
/// stay strings — mapping `dv` to a typed enum is the caller's business.
#[derive(Debug, Clone)]
pub struct FoundDevice {
    pub id: String,
    pub name: String,
    pub ips: Vec<String>,
    pub ipv6: Vec<String>,
    pub port: u16,
    pub device_type: String,
    pub version: String,
    pub platform: String,
}

/// Immutable mDNS info for one service instance, accumulated across packets.
#[derive(Debug, Clone)]
struct Instance {
    instance_fqdn: String,
    instance_name: String,
    id: String,
    port: u16,
    device_type: String,
    version: String,
    platform: String,
    target_hostname: String,
    ips: HashSet<String>,
    addrs_v6: HashSet<String>,
    txt_records: Vec<String>,
}

impl Instance {
    fn new(instance_fqdn: String, instance_name: String) -> Self {
        Instance {
            instance_fqdn,
            instance_name,
            id: String::new(),
            port: 0,
            device_type: String::new(),
            version: String::new(),
            platform: String::new(),
            target_hostname: String::new(),
            ips: HashSet::new(),
            addrs_v6: HashSet::new(),
            txt_records: Vec::new(),
        }
    }

    fn complete(&self) -> bool {
        !self.id.is_empty() && self.port > 0 && (!self.ips.is_empty() || !self.addrs_v6.is_empty())
    }
}

#[derive(Default)]
struct BrowserState {
    /// instanceFqdn(lower) → state
    instances: HashMap<String, Instance>,
    /// targetHostname(lower) → instanceFqdn(lower)
    hostname_to_instance: HashMap<String, String>,
    srv_txt_queried_at: HashMap<String, u64>,
    a_queried_at: HashMap<String, u64>,
}

struct Inner {
    state: Mutex<BrowserState>,
    running: AtomicBool,
    listener: Mutex<Option<host_responder::PacketListener>>,
    client_id: String,
    mdns_hostname: Arc<RwLock<String>>,
    /// Addresses injected by the caller (its own persistence — plain-rs stores
    /// nothing). Every browse cycle sends each one a directed unicast PTR
    /// requery so known devices stay discoverable when multicast is dead.
    seed_addrs: Mutex<HashSet<String>>,
    on_device: Box<dyn Fn(FoundDevice) + Send + Sync>,
    qu_active: AtomicBool,
    browse_cycles: AtomicU64,
}

/// mDNS service browser. Created once and cloned cheaply (all state behind an
/// `Arc`). The packet listener is resident for the whole session; the scan
/// loop is started/stopped on demand.
#[derive(Clone)]
pub struct MdnsServiceBrowser {
    inner: Arc<Inner>,
}

impl MdnsServiceBrowser {
    pub fn new(
        client_id: String,
        mdns_hostname: Arc<RwLock<String>>,
        on_device: impl Fn(FoundDevice) + Send + Sync + 'static,
    ) -> Self {
        MdnsServiceBrowser {
            inner: Arc::new(Inner {
                state: Mutex::new(BrowserState::default()),
                running: AtomicBool::new(false),
                listener: Mutex::new(None),
                client_id,
                mdns_hostname,
                seed_addrs: Mutex::new(HashSet::new()),
                on_device: Box::new(on_device),
                qu_active: AtomicBool::new(false),
                browse_cycles: AtomicU64::new(0),
            }),
        }
    }

    pub fn is_running(&self) -> bool {
        self.inner.running.load(Ordering::SeqCst)
    }

    /// Starts the periodic scan loop (idempotent). The loop self-terminates
    /// within one interval after [`MdnsServiceBrowser::stop`].
    pub fn start(&self) {
        self.install_listener();
        if self.inner.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let inner = self.inner.clone();
        std::thread::Builder::new()
            .name("plain-mdns-browser".to_string())
            .spawn(move || {
                while inner.running.load(Ordering::SeqCst) {
                    browse_once(&inner);
                    std::thread::sleep(Duration::from_millis(DISCOVER_INTERVAL_MS));
                }
            })
            .expect("spawn mdns browser");
    }

    /// Stops the periodic scan loop only. The packet listener and accumulated
    /// instance state stay installed: passive listening keeps refreshing
    /// paired peers' IPs after a network change even when no page is scanning.
    pub fn stop(&self) {
        self.inner.running.store(false, Ordering::SeqCst);
    }

    /// Installs the resident packet listener so inbound mDNS responses keep
    /// updating instance state (and paired peers' IP/port) even while the
    /// scan loop is stopped. Idempotent; installed once at first use.
    pub fn install_listener(&self) {
        let mut guard = self.inner.listener.lock().unwrap();
        if guard.is_some() {
            return;
        }
        let inner = self.inner.clone();
        let listener: host_responder::PacketListener =
            Arc::new(move |data: &[u8], sender: &str| handle_packet(&inner, data, sender));
        host_responder::add_packet_listener(listener.clone());
        *guard = Some(listener);
    }

    /// Replaces the requery seed addresses. Injected by the caller from its
    /// own persistence layer (e.g. a peers table); addresses learned at runtime
    /// from discovered instances are kept separately and merged automatically.
    pub fn seed_known_addrs(&self, addrs: &[String]) {
        let mut seed = self.inner.seed_addrs.lock().unwrap();
        seed.clear();
        seed.extend(addrs.iter().filter(|a| !a.is_empty()).cloned());
    }

    /// One-shot PTR query used by directed re-discovery of a paired peer.
    pub fn send_ptr_query(&self) {
        dispatch_query(&self.inner, PLAINAPP_SERVICE_TYPE, TYPE_PTR);
        dispatch_unicast_requery(&self.inner);
    }

    /// Read-only snapshot of every currently-known service instance.
    pub fn snapshot(&self) -> Vec<MdnsServiceSnapshot> {
        let state = self.inner.state.lock().unwrap();
        let mut list: Vec<MdnsServiceSnapshot> = state
            .instances
            .values()
            .map(|instance| MdnsServiceSnapshot {
                service_type: PLAINAPP_SERVICE_TYPE.to_string(),
                instance_name: instance.instance_name.clone(),
                instance_fqdn: instance.instance_fqdn.clone(),
                hostname: instance.target_hostname.clone(),
                port: instance.port,
                ips: instance.ips.iter().cloned().collect(),
                ipv6: instance.addrs_v6.iter().cloned().collect(),
                txt_records: instance.txt_records.clone(),
                complete: instance.complete(),
            })
            .collect();
        list.sort_by(|a, b| a.instance_fqdn.cmp(&b.instance_fqdn));
        list
    }

    /// Drops all accumulated instance state so the next browse cycle
    /// re-discovers every instance from scratch. Used after the local mDNS
    /// hostname changes so the snapshot no longer shows stale instances
    /// under the previous hostname.
    pub fn clear_instances(&self) {
        *self.inner.state.lock().unwrap() = BrowserState::default();
    }
}

/// Read-only mDNS details for one discovered service instance.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MdnsServiceSnapshot {
    pub service_type: String,
    pub instance_name: String,
    pub instance_fqdn: String,
    pub hostname: String,
    pub port: u16,
    pub txt_records: Vec<String>,
    pub ips: Vec<String>,
    pub ipv6: Vec<String>,
    pub complete: bool,
}

fn browse_once(inner: &Inner) {
    let cycle = inner.browse_cycles.fetch_add(1, Ordering::SeqCst) + 1;
    // QU fallback: sticky. Once activated it stays on — QU also works on
    // healthy networks (peers answer unicast), so switching back buys
    // nothing and would re-break discovery on flaky multicast.
    if !inner.qu_active.load(Ordering::SeqCst)
        && cycle >= QU_FALLBACK_AFTER_CYCLES
        && !host_responder::take_external_multicast_seen()
    {
        inner.qu_active.store(true, Ordering::SeqCst);
        log::info!("mdns browser: no external multicast, switching to QU queries");
    }
    // Self-heal after an external socket teardown (e.g. HTTP service stop):
    // the responder keeps packet listeners, so discovery resumes seamlessly.
    host_responder::ensure_started(&inner.mdns_hostname.read().unwrap());
    dispatch_query(inner, PLAINAPP_SERVICE_TYPE, TYPE_PTR);
    dispatch_unicast_requery(inner);
    // Follow up on instances that still lack port / metadata / IPs, re-asking
    // periodically because multicast responses can be dropped. Completed
    // instances refresh from every PTR announcement, which carries SRV/TXT/A
    // in its additional section (RFC 6763 §12).
    let now = now_ms();
    struct DueFollowUp {
        key: String,
        instance_name: String,
        target_hostname: String,
        srv_txt_due: bool,
        a_due: bool,
    }
    let due: Vec<DueFollowUp> = {
        let state = inner.state.lock().unwrap();
        state
            .instances
            .values()
            .filter(|instance| !instance.complete())
            .map(|instance| {
                let key = instance.instance_fqdn.clone();
                DueFollowUp {
                    srv_txt_due: now
                        .saturating_sub(*state.srv_txt_queried_at.get(&key).unwrap_or(&0))
                        >= FOLLOW_UP_RETRY_MS,
                    a_due: !instance.target_hostname.is_empty()
                        && now.saturating_sub(*state.a_queried_at.get(&key).unwrap_or(&0))
                            >= FOLLOW_UP_RETRY_MS,
                    key,
                    instance_name: instance.instance_name.clone(),
                    target_hostname: instance.target_hostname.clone(),
                }
            })
            .collect()
    };
    let (srv_txt_names, a_hostnames): (Vec<String>, Vec<String>) = {
        let mut state = inner.state.lock().unwrap();
        let mut srv_txt_names = Vec::new();
        let mut a_hostnames = Vec::new();
        for d in due {
            if d.srv_txt_due {
                state.srv_txt_queried_at.insert(d.key.clone(), now);
                srv_txt_names.push(d.instance_name);
            }
            if d.a_due {
                state.a_queried_at.insert(d.key, now);
                a_hostnames.push(d.target_hostname);
            }
        }
        (srv_txt_names, a_hostnames)
    };
    for hostname in a_hostnames {
        dispatch_query(inner, &hostname, TYPE_A);
    }
    for instance_name in srv_txt_names {
        dispatch_query(
            inner,
            &format!("{instance_name}.{PLAINAPP_SERVICE_TYPE}"),
            TYPE_SRV,
        );
        dispatch_query(
            inner,
            &format!("{instance_name}.{PLAINAPP_SERVICE_TYPE}"),
            TYPE_TXT,
        );
    }
}

/// Routes a query through the QU socket once multicast responses have proven
/// unreachable; unicast responses then arrive on the QU socket and reach
/// `handle_packet` through the shared packet listeners.
fn dispatch_query(inner: &Inner, name: &str, qtype: u16) {
    let qu = inner.qu_active.load(Ordering::SeqCst);
    let bytes = packet_codec::build_query(name, qtype, qu);
    if qu {
        host_responder::send_qu_query(&bytes);
    } else {
        host_responder::send_query(&bytes);
    }
}

/// Sends a QU (unicast-response) PTR query directly to every known address:
/// the injected seeds plus addresses learned from discovered instances. On
/// networks that drop mDNS multicast this is the only working discovery path —
/// peers answer with the full PTR+SRV+TXT+A record set in one packet
/// (RFC 6763 §12), which `handle_packet` digests like any multicast reply.
fn dispatch_unicast_requery(inner: &Inner) {
    let targets = {
        let seed = inner.seed_addrs.lock().unwrap().clone();
        let state = inner.state.lock().unwrap();
        unicast_targets(&seed, &state.instances)
    };
    if targets.is_empty() {
        return;
    }
    let bytes = packet_codec::build_query(PLAINAPP_SERVICE_TYPE, TYPE_PTR, true);
    for ip in targets {
        if host_responder::is_local_ip(&ip) || host_responder::is_local_ipv6(&ip) {
            continue;
        }
        host_responder::send_unicast_query(&bytes, &ip);
    }
}

/// Union of injected seed addresses and addresses learned from discovered
/// instances. Sorted so dispatch order is deterministic.
fn unicast_targets(
    seed_addrs: &HashSet<String>,
    instances: &HashMap<String, Instance>,
) -> Vec<String> {
    let mut targets: HashSet<String> = seed_addrs.clone();
    for instance in instances.values() {
        targets.extend(instance.ips.iter().cloned());
        targets.extend(instance.addrs_v6.iter().cloned());
    }
    let mut list: Vec<String> = targets.into_iter().collect();
    list.sort();
    list
}

fn handle_packet(inner: &Inner, data: &[u8], sender: &str) {
    // Ignore our own looped-back packets so we don't discover ourselves and
    // re-query our own SRV/TXT records on every discovery cycle.
    if host_responder::is_local_ip(sender) {
        return;
    }
    let Some(parsed) = packet_codec::parse_response(data) else {
        log::debug!("mdns browser: parse failed ({} bytes)", data.len());
        return;
    };
    if !parsed.is_response() {
        return;
    }
    let mut touched: HashSet<String> = HashSet::new();
    let mut discovered: Vec<String> = Vec::new(); // instances first seen in this packet
    // RFC 6762 §8.4 goodbye: TTL=0 records withdraw an instance — a peer
    // that renamed itself republishes under a new instance FQDN. Cached
    // entries never expire here, so without dropping the withdrawn one the
    // old name would be listed forever (next to the new one). Online status
    // is deliberately left untouched: the same id is re-announced under the
    // new name moments later, so clearing it would only flicker the UI.
    let all = parsed.all_records();
    let goodbye = goodbye_instance_keys(&all);
    // Withdrawn records must not be merged back in: find_instance recreates
    // a bare Instance for any name it does not know.
    let records: Vec<&super::service_info::MdnsRecord> = if goodbye.is_empty() {
        all
    } else {
        all.into_iter().filter(|r| r.ttl != 0).collect()
    };
    // Group this packet's A records by hostname first: a multi-homed host
    // (e.g. Wi-Fi + VPN) announces several addresses for the SAME hostname in
    // one packet and the whole set is authoritative — replacing per record
    // would keep only the last one (typically the VPN address).
    let a_by_hostname: HashMap<String, HashSet<String>> = {
        let mut grouped: HashMap<String, HashSet<String>> = HashMap::new();
        for record in &records {
            if record.record_type == TYPE_A {
                if let Some(ip) = record.ip() {
                    grouped
                        .entry(record.name.to_lowercase())
                        .or_default()
                        .insert(ip);
                }
            }
        }
        grouped
    };
    let aaaa_by_hostname: HashMap<String, HashSet<String>> = {
        let mut grouped: HashMap<String, HashSet<String>> = HashMap::new();
        for record in &records {
            if record.record_type == TYPE_AAAA {
                if let Some(ip) = record.ipv6() {
                    grouped
                        .entry(record.name.to_lowercase())
                        .or_default()
                        .insert(ip);
                }
            }
        }
        grouped
    };
    {
        let mut state = inner.state.lock().unwrap();
        if !goodbye.is_empty() {
            for key in &goodbye {
                state.instances.remove(key);
            }
            state
                .hostname_to_instance
                .retain(|_, v| !goodbye.contains(v));
        }
        for record in &records {
            match record.record_type {
                TYPE_PTR => {
                    if let Some(target) = record.ptr_target() {
                        if let Some((key, instance)) = find_instance(&state.instances, &target) {
                            if !state.instances.contains_key(&key) {
                                discovered.push(key.clone());
                            }
                            state.instances.insert(key.clone(), instance);
                            touched.insert(key);
                        }
                    }
                }
                TYPE_SRV => {
                    if let Some(srv) = record.srv() {
                        log::debug!("mdns browser: SRV {} port={}", record.name, srv.port);
                        if let Some((key, instance)) = find_instance(&state.instances, &record.name)
                        {
                            let mut updated = instance.clone();
                            updated.port = srv.port;
                            updated.target_hostname = srv.target.clone();
                            if !srv.target.is_empty() {
                                state
                                    .hostname_to_instance
                                    .insert(srv.target.to_lowercase(), key.clone());
                            }
                            state.instances.insert(key.clone(), updated);
                            touched.insert(key);
                        }
                    }
                }
                TYPE_TXT => {
                    if let Some(strings) = record.txt_strings() {
                        if let Some((key, instance)) = find_instance(&state.instances, &record.name)
                        {
                            let mut updated = instance.clone();
                            updated.txt_records = strings.clone();
                            for entry in &strings {
                                let Some(eq) = entry.find('=') else {
                                    continue;
                                };
                                if eq == 0 {
                                    continue;
                                }
                                let value = &entry[eq + 1..];
                                match &entry[..eq] {
                                    "id" => updated.id = value.to_string(),
                                    "dv" => updated.device_type = value.to_string(),
                                    "ver" => updated.version = value.to_string(),
                                    "pf" => updated.platform = value.to_string(),
                                    _ => {}
                                }
                            }
                            state.instances.insert(key.clone(), updated);
                            touched.insert(key);
                        }
                    }
                }
                _ => {}
            }
        }
        // The packet's A-record set is authoritative for the hostname's
        // CURRENT addresses. Applied once per packet, after SRV records may
        // have registered the hostname mapping: a host that moved networks
        // replaces the set (stale IP dropped), while a single packet carrying
        // several interface addresses keeps them all.
        for (hostname, ips) in a_by_hostname {
            if let Some(key) = state.hostname_to_instance.get(&hostname).cloned() {
                if let Some(instance) = state.instances.get_mut(&key) {
                    if instance.ips != ips {
                        instance.ips = ips;
                    }
                    touched.insert(key);
                }
            }
        }
        for (hostname, addrs) in aaaa_by_hostname {
            if let Some(key) = state.hostname_to_instance.get(&hostname).cloned() {
                if let Some(instance) = state.instances.get_mut(&key) {
                    if instance.addrs_v6 != addrs {
                        instance.addrs_v6 = addrs;
                    }
                    touched.insert(key);
                }
            }
        }
        // Drop the withdrawn instances' follow-up query timestamps too, so a
        // later republish under the same FQDN is re-resolved without waiting
        // out FOLLOW_UP_RETRY_MS.
        if !goodbye.is_empty() {
            for key in &goodbye {
                state.srv_txt_queried_at.remove(key);
                state.a_queried_at.remove(key);
            }
        }
    }

    // Immediately resolve newly discovered instances instead of waiting for
    // the next browse cycle (up to DISCOVER_INTERVAL_MS).
    let queries: Vec<String> = {
        let mut state = inner.state.lock().unwrap();
        let now = now_ms();
        discovered
            .into_iter()
            .filter_map(|key| {
                let (instance_name, complete) = {
                    let instance = state.instances.get(&key)?;
                    (instance.instance_name.clone(), instance.complete())
                };
                // Skip when the same packet already carried SRV/TXT.
                if complete {
                    return None;
                }
                state.srv_txt_queried_at.insert(key, now);
                Some(instance_name)
            })
            .collect()
    };
    for instance_name in queries {
        dispatch_query(
            inner,
            &format!("{instance_name}.{PLAINAPP_SERVICE_TYPE}"),
            TYPE_SRV,
        );
        dispatch_query(
            inner,
            &format!("{instance_name}.{PLAINAPP_SERVICE_TYPE}"),
            TYPE_TXT,
        );
    }

    let complete: Vec<Instance> = {
        let state = inner.state.lock().unwrap();
        touched
            .iter()
            // Skip our own looped-back announcements (multicast loop is
            // enabled on purpose so multiple same-device sockets keep
            // working) instead of emitting this device into the nearby
            // list / peer tables.
            .filter(|key| {
                state
                    .instances
                    .get(*key)
                    .map(|i| i.id != inner.client_id)
                    .unwrap_or(false)
            })
            .filter_map(|key| {
                let instance = state.instances.get(key)?;
                if instance.complete() {
                    Some(instance.clone())
                } else {
                    None
                }
            })
            .collect()
    };

    for instance in complete {
        let mut ips: Vec<String> = instance.ips.iter().cloned().collect();
        ips.sort();
        let mut ipv6: Vec<String> = instance.addrs_v6.iter().cloned().collect();
        ipv6.sort();
        (inner.on_device)(FoundDevice {
            id: instance.id,
            name: instance.instance_name,
            ips,
            ipv6,
            port: instance.port,
            device_type: instance.device_type,
            version: instance.version,
            platform: instance.platform,
        });
    }
}

/// Instances withdrawn by one packet's TTL=0 (goodbye) records, keyed like
/// the `instances` map. A PTR goodbye carries the instance in its rdata,
/// SRV/TXT in the record name. A records are hostname-scoped, so they never
/// withdraw an instance. Mirrors the Kotlin
/// `MdnsServiceBrowser.goodbyeInstanceKeys`.
fn goodbye_instance_keys(records: &[&super::service_info::MdnsRecord]) -> HashSet<String> {
    let mut keys = HashSet::new();
    for record in records {
        if record.ttl != 0 {
            continue;
        }
        let fqdn = match record.record_type {
            TYPE_PTR => record.ptr_target(),
            TYPE_SRV | TYPE_TXT => Some(record.name.clone()),
            _ => None,
        };
        if let Some(key) = fqdn.as_deref().and_then(instance_key_of) {
            keys.insert(key);
        }
    }
    keys
}

/// Resolves `name` against `current`; None when it is not one of our service
/// instances. Returns the key plus the existing or a fresh instance.
fn find_instance(current: &HashMap<String, Instance>, name: &str) -> Option<(String, Instance)> {
    let key = instance_key_of(name)?;
    let instance_name_len = name.len().saturating_sub(PLAINAPP_SERVICE_TYPE.len() + 1);
    let instance_name = name[..instance_name_len].to_string();
    Some((
        key.clone(),
        current
            .get(&key)
            .cloned()
            .unwrap_or_else(|| Instance::new(key, instance_name)),
    ))
}

/// `fqdn` as an instance key (lowercased) when it names one of our service
/// instances. Mirrors the Kotlin `MdnsServiceBrowser.instanceKeyOf`.
fn instance_key_of(fqdn: &str) -> Option<String> {
    if !fqdn
        .to_lowercase()
        .ends_with(&PLAINAPP_SERVICE_TYPE.to_lowercase())
    {
        return None;
    }
    let instance_name_len = fqdn.len().saturating_sub(PLAINAPP_SERVICE_TYPE.len() + 1);
    if instance_name_len == 0 {
        return None;
    }
    Some(fqdn.to_lowercase())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_instance(fqdn: &str) -> (String, Instance) {
        let key = fqdn.to_lowercase();
        let name = fqdn
            .strip_suffix(&format!(".{PLAINAPP_SERVICE_TYPE}"))
            .unwrap();
        (key.clone(), Instance::new(key, name.to_string()))
    }

    #[test]
    fn find_instance_matches_service_type_only() {
        let mut map = HashMap::new();
        let (key, inst) = test_instance("Pixel 7._plainapp._tcp.local");
        map.insert(key.clone(), inst);

        let (k, i) = find_instance(&map, "pixel 7._plainapp._tcp.local").unwrap();
        assert_eq!(k, key);
        assert_eq!(i.instance_name, "Pixel 7");

        // Non-service names are ignored.
        assert!(find_instance(&map, "other._http._tcp.local").is_none());
        // The bare service type has no instance name.
        assert!(find_instance(&map, PLAINAPP_SERVICE_TYPE).is_none());
    }

    #[test]
    fn instance_complete_requires_all_parts() {
        let (_key, mut inst) = test_instance("X._plainapp._tcp.local");
        assert!(!inst.complete());
        inst.id = "abc".to_string();
        assert!(!inst.complete());
        inst.port = 8443;
        assert!(!inst.complete());
        inst.ips.insert("192.168.1.2".to_string());
        assert!(inst.complete());
    }

    #[test]
    fn clear_instances_resets_accumulated_state() {
        let browser =
            MdnsServiceBrowser::new(String::new(), Arc::new(RwLock::new(String::new())), |_| {});
        {
            let mut state = browser.inner.state.lock().unwrap();
            state.instances.insert(
                "Pixel 7._plainapp._tcp.local".to_string(),
                Instance::new(
                    "Pixel 7._plainapp._tcp.local".to_string(),
                    "Pixel 7".to_string(),
                ),
            );
        }
        browser.clear_instances();
        assert!(browser.inner.state.lock().unwrap().instances.is_empty());
    }

    #[test]
    fn stop_keeps_instance_state_for_resident_listening() {
        let browser =
            MdnsServiceBrowser::new(String::new(), Arc::new(RwLock::new(String::new())), |_| {});
        {
            let mut state = browser.inner.state.lock().unwrap();
            let mut inst = Instance::new("p9._plainapp._tcp.local".to_string(), "p9".to_string());
            inst.id = "1xvuvk3ujzxyn".to_string();
            inst.port = 8443;
            inst.ips.insert("192.168.1.20".to_string());
            state.instances.insert(inst.instance_fqdn.clone(), inst);
        }
        browser.stop();
        assert!(!browser.inner.state.lock().unwrap().instances.is_empty());
    }

    #[test]
    fn a_record_replaces_stale_ip_instead_of_accumulating() {
        let browser =
            MdnsServiceBrowser::new(String::new(), Arc::new(RwLock::new(String::new())), |_| {});
        let key = "p9._plainapp._tcp.local";
        {
            let mut state = browser.inner.state.lock().unwrap();
            let mut inst = Instance::new(key.to_string(), "p9".to_string());
            inst.id = "1xvuvk3ujzxyn".to_string();
            inst.target_hostname = "p9.local".to_string();
            inst.port = 8443;
            inst.ips.insert("192.168.1.10".to_string());
            state.instances.insert(key.to_string(), inst);
            state
                .hostname_to_instance
                .insert("p9.local".to_string(), key.to_string());
        }
        // The host announces its NEW address after moving networks.
        let query = super::packet_codec::build_query("p9.local", TYPE_A, false);
        let response = super::packet_codec::build_response_if_match(
            &query,
            "p9.local",
            &["192.168.1.20".to_string()],
        )
        .expect("a-record response");
        handle_packet(&browser.inner, &response.bytes, "192.0.2.1");
        let state = browser.inner.state.lock().unwrap();
        let inst = state.instances.get(key).expect("instance");
        let ips: Vec<String> = inst.ips.iter().cloned().collect();
        assert_eq!(ips, vec!["192.168.1.20".to_string()]);
    }

    #[test]
    fn multi_a_records_in_one_packet_keep_all_interface_ips() {
        let browser =
            MdnsServiceBrowser::new(String::new(), Arc::new(RwLock::new(String::new())), |_| {});
        let key = "p9._plainapp._tcp.local";
        {
            let mut state = browser.inner.state.lock().unwrap();
            let mut inst = Instance::new(key.to_string(), "p9".to_string());
            inst.id = "1xvuvk3ujzxyn".to_string();
            inst.target_hostname = "p9.local".to_string();
            inst.port = 8443;
            inst.ips.insert("192.168.1.10".to_string());
            state.instances.insert(key.to_string(), inst);
            state
                .hostname_to_instance
                .insert("p9.local".to_string(), key.to_string());
        }
        // A multi-homed host (Wi-Fi + VPN) answers with BOTH addresses in one
        // packet — the set is authoritative, so both are kept instead of the
        // last record (the VPN address) winning.
        let query = super::packet_codec::build_query("p9.local", TYPE_A, false);
        let response = super::packet_codec::build_response_if_match(
            &query,
            "p9.local",
            &["192.168.1.10".to_string(), "10.8.0.2".to_string()],
        )
        .expect("a-record response");
        handle_packet(&browser.inner, &response.bytes, "192.0.2.1");
        let state = browser.inner.state.lock().unwrap();
        let inst = state.instances.get(key).expect("instance");
        let mut ips: Vec<String> = inst.ips.iter().cloned().collect();
        ips.sort();
        assert_eq!(
            ips,
            vec!["10.8.0.2".to_string(), "192.168.1.10".to_string()]
        );
    }

    // ── goodbye_instance_keys (RFC 6762 §8.4) ─────────────────────────────
    // A renamed peer withdraws its old instance with a TTL=0 packet; the
    // browser must drop that instance instead of listing the old name forever.

    use super::super::service_info::{MdnsServiceInfo, PLAINAPP_SERVICE_TYPE as SERVICE_TYPE};
    use super::super::service_response_builder;

    fn test_service(instance_name: &str, service_type: &str) -> MdnsServiceInfo {
        MdnsServiceInfo {
            instance_name: instance_name.to_string(),
            service_type: service_type.to_string(),
            target_hostname: "plainapp-abc.local".to_string(),
            port: 8443,
            txt_records: vec!["id=abc".to_string()],
            ips: vec!["192.168.1.50".to_string()],
        }
    }

    fn goodbye_keys(bytes: &[u8]) -> HashSet<String> {
        let parsed = packet_codec::parse_response(bytes).expect("parse");
        goodbye_instance_keys(&parsed.all_records())
    }
    #[test]
    fn goodbye_packet_withdraws_the_renamed_instance() {
        let browser =
            MdnsServiceBrowser::new(String::new(), Arc::new(RwLock::new(String::new())), |_| {});
        let key = "pixel 7 pro._plainapp._tcp.local";
        {
            let mut state = browser.inner.state.lock().unwrap();
            state.instances.insert(
                key.to_string(),
                Instance::new(key.to_string(), "Pixel 7 Pro".to_string()),
            );
            state
                .hostname_to_instance
                .insert("plainapp-abc.local".to_string(), key.to_string());
            state.srv_txt_queried_at.insert(key.to_string(), 42);
        }
        let goodbye =
            service_response_builder::build_goodbye(&test_service("Pixel 7 Pro", SERVICE_TYPE));
        handle_packet(&browser.inner, &goodbye, "192.0.2.1");
        let state = browser.inner.state.lock().unwrap();
        assert!(!state.instances.contains_key(key));
        assert!(
            !state
                .hostname_to_instance
                .contains_key("plainapp-abc.local")
        );
        assert!(!state.srv_txt_queried_at.contains_key(key));
    }

    #[test]
    fn records_with_a_live_ttl_are_not_a_goodbye() {
        let query = packet_codec::build_ptr_query(SERVICE_TYPE);
        let response = service_response_builder::build_response_if_match(
            &query,
            &test_service("Pixel 7 Pro", SERVICE_TYPE),
        )
        .expect("response");
        assert!(goodbye_keys(&response.bytes).is_empty());
    }

    #[test]
    fn goodbye_for_another_service_type_is_ignored() {
        let other = test_service("Speaker", "_airplay._tcp.local");
        let goodbye = service_response_builder::build_goodbye(&other);
        assert!(goodbye_keys(&goodbye).is_empty());
    }

    #[test]
    fn zero_ttl_a_record_does_not_withdraw_an_instance() {
        // Build a goodbye-shaped packet, then swap the SRV/TXT records for a
        // TTL=0 A record: hostname-scoped records never withdraw an instance.
        let browser =
            MdnsServiceBrowser::new(String::new(), Arc::new(RwLock::new(String::new())), |_| {});
        let key = "p9._plainapp._tcp.local";
        {
            let mut state = browser.inner.state.lock().unwrap();
            state.instances.insert(
                key.to_string(),
                Instance::new(key.to_string(), "p9".to_string()),
            );
        }
        // A zero-TTL A record for the instance's target hostname.
        let mut out = Vec::new();
        packet_codec::write_header(&mut out, 1, 0);
        packet_codec::write_record(
            &mut out,
            &packet_codec::encode_name("p9.local"),
            TYPE_A,
            packet_codec::DNS_CACHE_FLUSH_CLASS_IN,
            0,
            &packet_codec::ip_to_bytes("192.168.1.10"),
        );
        handle_packet(&browser.inner, &out, "192.0.2.1");
        assert!(
            browser
                .inner
                .state
                .lock()
                .unwrap()
                .instances
                .contains_key(key)
        );
    }

    #[test]
    fn self_looped_packet_is_ignored() {
        let response = {
            let query = packet_codec::build_query("p9.local", TYPE_A, false);
            packet_codec::build_response_if_match(&query, "p9.local", &["192.168.1.20".to_string()])
                .expect("a-record response")
        };
        let remote =
            MdnsServiceBrowser::new(String::new(), Arc::new(RwLock::new(String::new())), |_| {});
        let local =
            MdnsServiceBrowser::new(String::new(), Arc::new(RwLock::new(String::new())), |_| {});
        let seed = |browser: &MdnsServiceBrowser| {
            let key = "p9._plainapp._tcp.local";
            let mut state = browser.inner.state.lock().unwrap();
            let mut inst = Instance::new(key.to_string(), "p9".to_string());
            inst.port = 8443;
            inst.ips.insert("192.168.1.10".to_string());
            state.instances.insert(key.to_string(), inst);
            state
                .hostname_to_instance
                .insert("p9.local".to_string(), key.to_string());
        };
        seed(&remote);
        seed(&local);

        handle_packet(&remote.inner, &response.bytes, "192.0.2.1");
        assert!(
            remote
                .inner
                .state
                .lock()
                .unwrap()
                .instances
                .values()
                .all(|i| i.ips.contains(&"192.168.1.20".to_string()))
        );

        if let Some(ip) = local_ip_sender() {
            handle_packet(&local.inner, &response.bytes, &ip);
            assert!(
                local
                    .inner
                    .state
                    .lock()
                    .unwrap()
                    .instances
                    .values()
                    .all(|i| i.ips.contains(&"192.168.1.10".to_string())
                        && !i.ips.contains(&"192.168.1.20".to_string()))
            );
        }
    }

    #[test]
    fn unicast_targets_merges_seed_and_learned_addrs() {
        let mut seed = HashSet::new();
        seed.insert("192.168.1.30".to_string());
        let mut instances = HashMap::new();
        let mut inst = Instance::new("p9._plainapp._tcp.local".to_string(), "p9".to_string());
        inst.ips.insert("192.168.1.20".to_string());
        inst.ips.insert("192.168.1.30".to_string());
        inst.addrs_v6.insert("fd00::9".to_string());
        instances.insert(inst.instance_fqdn.clone(), inst);
        let targets = unicast_targets(&seed, &instances);
        assert_eq!(targets, vec!["192.168.1.20", "192.168.1.30", "fd00::9"]);
    }

    #[test]
    fn unicast_targets_empty_when_nothing_known() {
        assert!(unicast_targets(&HashSet::new(), &HashMap::new()).is_empty());
    }

    #[test]
    fn seed_known_addrs_replaces_previous_seed_and_skips_empty() {
        let browser =
            MdnsServiceBrowser::new(String::new(), Arc::new(RwLock::new(String::new())), |_| {});
        browser.seed_known_addrs(&["10.0.0.5".to_string(), "10.0.0.6".to_string()]);
        browser.seed_known_addrs(&["192.168.1.2".to_string(), String::new()]);
        let seed = browser.inner.seed_addrs.lock().unwrap();
        assert_eq!(seed.len(), 1);
        assert!(seed.contains("192.168.1.2"));
    }

    fn local_ip_sender() -> Option<String> {
        super::host_responder::local_ipv4_strs().into_iter().next()
    }
}
