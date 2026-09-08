//! Discovery manager over mDNS — mirrors plain-app `MdnsDiscoverManager`.
//!
//! Replaces the old LAN discovery (custom UDP multicast on
//! `224.0.0.100:52352`). Publishing the `_plainapp._tcp.local` service is
//! driven by the HTTPS server lifecycle (`LocalServerState::rebind` calls
//! [`publish_service`]), while this manager guarantees the shared responder
//! socket is up so the browser can send queries, and owns the browser
//! lifecycle.
//!
//! Pairing is handled over HTTPS via the `POST /nearby` REST endpoint
//! instead of UDP (see `local::pairing`).

use super::peer_status_manager::PeerStatusManager;
use crate::local::db::{ChatDb, DPeer, now_iso};
use crate::local::enums::DeviceType;
use crate::local::graphql::schema::types::Peer;
use crate::local::graphql::{
    WS_NEARBY_DEVICE_FOUND, WS_NEARBY_DISCOVERY_STARTED, WS_NEARBY_DISCOVERY_STOPPED, WsEvent,
};
use crate::local::pairing::PairingManager;
use crate::prefs::AppIdentity;
use plain_rs::mdns::host_responder;
use plain_rs::mdns::service_browser::{FoundDevice, MdnsServiceBrowser, MdnsServiceSnapshot};
use serde::Serialize;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{
    Arc, Mutex, RwLock,
    atomic::{AtomicU16, Ordering},
};
use tokio::sync::broadcast;

const LOCAL_DEVICE_TYPE_WIRE: &str = "COMPUTER";

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredDevice {
    pub id: String,
    pub name: String,
    pub ips: Vec<String>,
    pub port: u16,
    pub device_type: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub last_seen: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub discovery_methods: Vec<String>,
}

fn split_host(host: &str) -> (String, u16) {
    match host.rsplit_once(':') {
        Some((ip, port)) => match port.parse::<u16>() {
            Ok(p) => (ip.to_string(), p),
            Err(_) => (host.to_string(), 8443),
        },
        None => (host.to_string(), 8443),
    }
}

#[derive(Clone)]
pub struct NearbyDiscoverManager {
    db: Arc<ChatDb>,
    identity: Arc<AppIdentity>,
    device_name: Arc<RwLock<String>>,
    mdns_hostname: Arc<RwLock<String>>,
    pairing: PairingManager,
    peer_status: PeerStatusManager,
    https_port: Arc<AtomicU16>,
    event_tx: Arc<RwLock<Option<broadcast::Sender<WsEvent>>>>,
    app_handle: Arc<RwLock<Option<tauri::AppHandle>>>,
    browser: MdnsServiceBrowser,
    seen_in_session: Arc<Mutex<HashMap<String, DiscoveredDevice>>>,
    found_tx: std::sync::mpsc::Sender<FoundDevice>,
    app_version: String,
}

impl NearbyDiscoverManager {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: Arc<ChatDb>,
        identity: Arc<AppIdentity>,
        device_name: Arc<RwLock<String>>,
        mdns_hostname: Arc<RwLock<String>>,
        pairing: PairingManager,
        peer_status: PeerStatusManager,
        https_port: u16,
        app_version: String,
    ) -> Self {
        let (found_tx, found_rx) = std::sync::mpsc::channel::<FoundDevice>();
        let this = NearbyDiscoverManager {
            db,
            identity,
            device_name,
            mdns_hostname,
            pairing,
            peer_status,
            https_port: Arc::new(AtomicU16::new(https_port)),
            event_tx: Arc::new(RwLock::new(None)),
            app_handle: Arc::new(RwLock::new(None)),
            browser: MdnsServiceBrowser::new(
                String::new(),
                Arc::new(RwLock::new(String::new())),
                |_| {},
            ),
            seen_in_session: Arc::new(Mutex::new(HashMap::new())),
            found_tx,
            app_version,
        };
        let worker = this.clone();
        std::thread::Builder::new()
            .name("nearby-discover-worker".into())
            .spawn(move || {
                while let Ok(device) = found_rx.recv() {
                    worker.process_discovered_device(device);
                }
            })
            .expect("spawn nearby-discover-worker");
        let callback_state = this.clone();
        let browser = MdnsServiceBrowser::new(
            this.identity.client_id.clone(),
            this.mdns_hostname.clone(),
            move |device: FoundDevice| callback_state.on_device_found(device),
        );
        Self { browser, ..this }
    }

    pub fn set_event_tx(&self, event_tx: broadcast::Sender<WsEvent>) {
        *self.event_tx.write().unwrap() = Some(event_tx);
    }

    pub fn set_app_handle(&self, handle: tauri::AppHandle) {
        *self.app_handle.write().unwrap() = Some(handle);
    }

    /// Called by `LocalServerState::rebind` once the HTTPS port is bound.
    /// Also (re)publishes the `_plainapp._tcp.local` service so the desktop
    /// is discoverable by phones — mirrors plain-app's `NsdHelper.registerService`.
    pub fn set_https_port(&self, port: u16) {
        self.https_port.store(port, Ordering::SeqCst);
        self.publish_service();
    }

    /// Advertises the PlainApp service on the shared mDNS responder. The
    /// instance name is the device name; TXT records carry the identity
    /// (id / device type / version / platform).
    pub fn publish_service(&self) {
        let hostname = self.mdns_hostname();
        let port = self.https_port.load(Ordering::SeqCst);
        let service = (port > 0).then(|| {
            plain_rs::mdns::service_info::build_service_info(
                &self.device_name.read().unwrap().clone(),
                &hostname,
                port,
                &self.identity.client_id,
                LOCAL_DEVICE_TYPE_WIRE,
                &self.app_version,
                std::env::consts::OS,
                host_responder::local_ipv4_strs(),
            )
        });
        host_responder::start(&hostname, service);
    }

    /// Republishes the local service after the advertised data changed (device
    /// renamed, port changed) so peers pick it up without waiting for the next
    /// re-announce. No-op while no service is published (web service off).
    /// Mirrors plain-app's `MdnsDiscoverManager.updateAdvertisedService`.
    pub fn update_advertised_service(&self) {
        let port = self.https_port.load(Ordering::SeqCst);
        if port == 0 {
            return;
        }
        let service = plain_rs::mdns::service_info::build_service_info(
            &self.device_name.read().unwrap().clone(),
            &self.mdns_hostname(),
            port,
            &self.identity.client_id,
            LOCAL_DEVICE_TYPE_WIRE,
            &self.app_version,
            std::env::consts::OS,
            host_responder::local_ipv4_strs(),
        );
        host_responder::update_service(service);
    }

    /// Applies a device rename: updates the shared name and republishes the
    /// mDNS service so peers drop the old instance (goodbye) and see the new
    /// name right away. The rename entry points (GraphQL `updateDeviceName`
    /// and the `set_device_name` command) share this path. Persistence is the
    /// caller's business (`prefs::set_device_name`).
    pub fn apply_device_rename(&self, name: &str) {
        *self.device_name.write().unwrap() = name.to_string();
        self.update_advertised_service();
    }

    /// Ensures the shared mDNS responder socket is up so the browser can
    /// send queries and the responder can answer PTR/SRV/TXT/A queries.
    /// Service registration itself happens with the HTTPS server lifecycle.
    /// The browser's packet listener is installed here as well and never
    /// removed: passive listening stays resident for the whole session so a
    /// paired peer's IP change is picked up without any page scanning.
    pub fn start(&self) {
        host_responder::ensure_started(&self.mdns_hostname());
        self.browser
            .seed_known_addrs(&peer_seed_addrs(&self.db.get_peers()));
        self.browser.install_listener();
    }

    /// Current mDNS hostname — mirrors plain-app's `TempData.mdnsHostname`.
    pub fn mdns_hostname(&self) -> String {
        self.mdns_hostname.read().unwrap().clone()
    }

    /// Persists and applies a new mDNS hostname — mirrors plain-app's
    /// `MdnsHostnamePreference` + `WebAddressBar` save path, applied
    /// immediately by re-publishing on the shared responder socket.
    pub fn set_mdns_hostname(&self, handle: &tauri::AppHandle, hostname: &str) {
        crate::prefs::set_mdns_hostname(handle, hostname);
        *self.mdns_hostname.write().unwrap() = hostname.to_string();
        self.publish_service();
        // Drop instances cached under the previous hostname and re-browse so
        // the debug snapshot reflects the change instead of showing stale data.
        self.browser.clear_instances();
        self.browser.send_ptr_query();
    }

    /// Read-only snapshot of every known `_plainapp._tcp.local` instance —
    /// mirrors plain-app's `MdnsServiceBrowser.snapshot` (`MdnsDebugPage`).
    pub fn mdns_snapshot(&self) -> Vec<MdnsServiceSnapshot> {
        self.browser.snapshot()
    }

    /// Mirrors plain-app's `startDiscovery` mutation: runs the mDNS browser
    /// loop that pushes discovered devices over the local server WS as
    /// `WS_NEARBY_DEVICE_FOUND`.
    ///
    /// `seen_in_session` is always cleared — even when the scan loop is
    /// already running — so opening the discovery UI re-emits the current
    /// device set. Without this, the first open after app start shows an
    /// empty list: devices announced during the initial window were emitted
    /// (and deduped) before the frontend was listening, and the dedup here
    /// suppresses re-emitting them.
    pub fn start_discovery(&self) -> bool {
        let already_running = self.browser.is_running();
        self.start();
        {
            let mut seen = self.seen_in_session.lock().unwrap();
            seen.clear();
        }
        self.emit_event(WS_NEARBY_DISCOVERY_STARTED, "{}");
        self.browser.start();
        !already_running
    }

    /// Mirrors plain-app's `stopDiscovery` mutation.
    pub fn stop_discovery(&self) -> bool {
        if !self.browser.is_running() {
            return false;
        }
        self.browser.stop();
        {
            let mut seen = self.seen_in_session.lock().unwrap();
            seen.clear();
        }
        self.emit_event(WS_NEARBY_DISCOVERY_STOPPED, "{}");
        true
    }

    /// Mirrors plain-app's `isDiscovering` query.
    pub fn is_discovering(&self) -> bool {
        self.browser.is_running()
    }

    /// Triggers an immediate one-shot mDNS PTR browse. Responses for a paired
    /// peer refresh its IP/port via [`Self::update_known_peer`], letting
    /// `PeerStatusManager` detect whether the reply arrived within its wait
    /// window. Mirrors plain-app's `MdnsDiscoverManager.browse`.
    pub fn browse(&self) {
        // The resident packet listener (installed at app start) parses the
        // replies and refreshes the peer row; no scan loop needed here, so the
        // nearby list stays quiet unless a page is actually discovering.
        let known = self.browser.snapshot().len();
        log::debug!(
            "mdns browse: one-shot PTR for {known} known instances, responder running={}",
            host_responder::is_running()
        );
        self.start();
        self.browser.send_ptr_query();
    }

    /// Current `ip:port` of a paired peer straight from the peers table.
    /// The resident mDNS listener keeps it fresh from the peer's own
    /// announcements, so host healing does not depend on an outgoing
    /// multicast query actually reaching the peer.
    pub fn peer_address(&self, id: &str) -> Option<String> {
        let peer = self
            .db
            .get_peer_by_id(id)
            .filter(|p| p.is_paired() || !p.token.is_empty())?;
        if peer.ip.is_empty() || peer.port == 0 {
            log::debug!("peer_address {}: db row has no address", id);
            return None;
        }
        let addr = format!("{}:{}", peer.best_ip(), peer.port);
        log::debug!("peer_address {} -> {}", id, addr);
        Some(addr)
    }

    /// Records a successful remote-device login (see `ChatDb::login_peer`).
    pub fn login_peer(
        &self,
        id: &str,
        name: &str,
        host: &str,
        device_type: DeviceType,
        token: &str,
        signature_public_key: &str,
    ) {
        let (ip, port) = split_host(host);
        self.db
            .login_peer(id, name, &ip, port, device_type, token, signature_public_key);
    }

    pub fn logout_peer(&self, id: &str) {
        self.db.logout_peer(id);
    }

    pub fn update_peer_name(&self, id: &str, name: &str) {
        self.db.update_peer_name(id, name);
    }

    /// Peers with an active login token — the device-switcher list.
    pub fn login_peers(&self) -> Vec<Peer> {
        self.db
            .get_login_peers()
            .into_iter()
            .map(|p| {
                let online = self.peer_status.is_online(&p.id);
                Peer::from_dpeer(p, online)
            })
            .collect()
    }

    fn emit_event(&self, event_type: i32, payload: &str) {
        if let Some(tx) = self.event_tx.read().unwrap().clone() {
            let _ = tx.send(WsEvent {
                event_type,
                payload: payload.to_string(),
            });
        }
    }

    /// Browser callback — runs on the mDNS responder packet thread, so it
    /// must never block: the device is handed to the dedicated worker thread
    /// ([`Self::process_discovered_device`]) which owns all DB access and
    /// event emission. Mirrors plain-app's `MdnsServiceBrowser.emitDevice`
    /// (`NearbyViewModel.handleNewDevice` + `PeerManager.applyDeviceDiscovered`).
    fn on_device_found(&self, device: FoundDevice) {
        let _ = self.found_tx.send(device);
    }

    /// Worker-thread half of [`Self::on_device_found`].
    fn process_discovered_device(&self, device: FoundDevice) {
        // Resident-listener path: always refresh a paired peer's address so a
        // changed IP is picked up by the next reconnect attempt even while
        // the nearby scan loop is off.
        self.update_known_peer(&device);
        self.peer_status.set_online(&device.id, true);
        let mut ips = device.ips.clone();
        ips.sort();
        let discovered = DiscoveredDevice {
            id: device.id.clone(),
            name: device.name.clone(),
            ips,
            port: device.port,
            device_type: device.device_type.clone(),
            version: device.version.clone(),
            platform: device.platform.clone(),
            last_seen: now_iso(),
            status: self.get_device_status(&device.id),
            discovery_methods: vec!["LAN".to_string()],
        };
        // mDNS announcements repeat every second; emit only on change.
        let changed = {
            let mut seen = self.seen_in_session.lock().unwrap();
            match seen.get(&discovered.id) {
                Some(prev) if same_snapshot(prev, &discovered) => false,
                _ => {
                    seen.insert(discovered.id.clone(), discovered.clone());
                    true
                }
            }
        };
        if !changed {
            return;
        }
        log::debug!(
            "nearby device changed: id={} ips={:?} port={}",
            device.id,
            device.ips,
            device.port
        );
        self.emit_event(
            WS_NEARBY_DEVICE_FOUND,
            &serde_json::to_string(&discovered).unwrap_or_default(),
        );
    }

    /// Refreshes a known peer's address info from an mDNS response — mirrors
    /// plain-app's `PeerManager.applyDeviceDiscovered` (bumps `updatedAt`).
    fn update_known_peer(&self, device: &FoundDevice) {
        // Paired peers (chat) and logged-in peers (token) both track the
        // device address; unrelated peers are left untouched.
        let Some(mut peer) = self
            .db
            .get_peer_by_id(&device.id)
            .filter(|p| p.is_paired() || !p.token.is_empty())
        else {
            log::debug!("update_known_peer: {} not paired/logged-in, skip", device.id);
            return;
        };
        // mDNS announcements repeat every few seconds — skip the write when
        // nothing changed so the peers table isn't hammered by upserts.
        let ip = device.ips.join(",");
        let device_type =
            DeviceType::from_str(&device.device_type).unwrap_or(DeviceType::Other);
        if peer.name == device.name
            && peer.ip == ip
            && peer.port == device.port
            && peer.device_type == device_type
        {
            return;
        }
        let old_addr = format!("{}:{}", peer.best_ip(), peer.port);
        log::info!("update_known_peer: {} address {} -> {}", device.id, old_addr, ip);
        peer.name = device.name.clone();
        peer.ip = ip;
        peer.port = device.port;
        peer.device_type = device_type;
        peer.updated_at = now_iso();
        self.db.upsert_peer(&peer);
        if let (Some(handle), Some(ip)) = (
            self.app_handle.read().unwrap().clone(),
            device.ips.iter().min(),
        ) {
            use tauri::Emitter;
            let _ = handle.emit(
                "device-host-changed",
                serde_json::json!({ "clientId": device.id, "host": format!("{ip}:{}", device.port) }),
            );
        }
    }

    /// Mirrors plain-app's `NearbyViewModel.getStatus(deviceId, paired)`:
    /// PAIRING if a pairing session is in flight, else PAIRED if the peer
    /// exists in the DB with Paired status, else UNPAIRED.
    fn get_device_status(&self, device_id: &str) -> String {
        if self.pairing.is_pairing(device_id) {
            return "PAIRING".to_string();
        }
        match self.db.get_peer_by_id(device_id) {
            Some(peer) if peer.is_paired() => "PAIRED".to_string(),
            _ => "UNPAIRED".to_string(),
        }
    }
}

/// Equality over the stable fields of a discovered device — `last_seen` is
/// excluded so repeated mDNS announcements of unchanged data emit once.
fn same_snapshot(a: &DiscoveredDevice, b: &DiscoveredDevice) -> bool {
    a.id == b.id
        && a.name == b.name
        && a.ips == b.ips
        && a.port == b.port
        && a.device_type == b.device_type
        && a.version == b.version
        && a.platform == b.platform
        && a.status == b.status
}

/// Addresses of paired / logged-in peers — the requery seeds for the mDNS
/// browser's directed unicast path. Networks that silently drop multicast
/// (VPN / AP isolation) stay discoverable this way; peers without an address
/// contribute nothing.
fn peer_seed_addrs(peers: &[DPeer]) -> Vec<String> {
    peers
        .iter()
        .filter(|p| (p.is_paired() || !p.token.is_empty()) && !p.ip.is_empty())
        .flat_map(|p| p.ip.split(',').map(str::trim).map(str::to_string))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str, ip: &str, status: &str) -> DiscoveredDevice {
        DiscoveredDevice {
            id: id.to_string(),
            name: "Pixel 7".to_string(),
            ips: vec![ip.to_string()],
            port: 8443,
            device_type: "PHONE".to_string(),
            version: "1.0".to_string(),
            platform: "android".to_string(),
            last_seen: String::new(),
            status: status.to_string(),
            discovery_methods: vec!["LAN".to_string()],
        }
    }

    #[test]
    fn same_snapshot_ignores_last_seen_but_detects_changes() {
        let a = device("d1", "192.168.1.2", "UNPAIRED");
        let mut b = a.clone();
        assert!(same_snapshot(&a, &b));
        b.last_seen = "2026-08-18T00:00:00Z".to_string();
        assert!(same_snapshot(&a, &b), "last_seen alone must not re-emit");
        b.ips = vec!["192.168.1.3".to_string()];
        assert!(!same_snapshot(&a, &b), "ip change must re-emit");
        b.ips = a.ips.clone();
        b.status = "PAIRED".to_string();
        assert!(!same_snapshot(&a, &b), "status change must re-emit");
    }

    fn seed_peer(id: &str, ip: &str, paired: bool, token: &str) -> DPeer {
        let mut peer = DPeer::new(id, id, ip, 8443, DeviceType::Phone);
        if paired {
            peer.status = crate::local::enums::PeerStatus::Paired;
        }
        peer.token = token.to_string();
        peer
    }

    #[test]
    fn peer_seed_addrs_takes_addresses_of_paired_or_logged_in_peers() {
        let peers = vec![
            seed_peer("p1", "192.168.1.10", true, ""),
            seed_peer("p2", "192.168.1.11, 192.168.2.11", false, "tok"),
            seed_peer("p3", "192.168.1.12", false, ""),
            seed_peer("p4", "", true, ""),
        ];
        assert_eq!(
            peer_seed_addrs(&peers),
            vec![
                "192.168.1.10".to_string(),
                "192.168.1.11".to_string(),
                "192.168.2.11".to_string()
            ]
        );
    }
}

