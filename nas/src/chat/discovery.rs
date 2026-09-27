//! NAS LAN discovery over the shared plain_rs mDNS stack — port of
//! plain-desktop's NearbyDiscoverManager, NAS-scoped:
//!
//! * advertise `_plainapp._tcp.local` (device type `NAS`) so phones and
//!   desktops find this server for pairing;
//! * browse residently: refresh paired peers' addresses when they
//!   re-announce (IP changes), mark them online, keep the
//!   nearby-device cache warm.
//!
//! The GraphQL `peers.online` flag reads the online set; the browse data
//! itself is available to a future discovery surface (contract question,
//! see docs/api/chat.md).

use std::collections::HashSet;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use plain_rs::mdns::host_responder;
use plain_rs::mdns::service_browser::{FoundDevice, MdnsServiceBrowser};
use plain_rs::mdns::service_info::build_service_info;

use plain_rs::chat::db::ChatDb;
use plain_rs::chat::service::ChatIdentity;

const LOCAL_DEVICE_TYPE_WIRE: &str = "NAS";
/// Mirrors plain-app's `MdnsHostnamePreference`: random two-char label
/// from the DNS-safe alphabet, persisted on first use.
const MDNS_HOSTNAME_CHARS: &[u8] = b"abcdefghjkmnpqrstuwxyz";

fn ensure_mdns_hostname(prefs: &crate::prefs::Prefs) -> String {
    if let Some(hostname) = prefs
        .get::<String>("mdns_hostname")
        .ok()
        .flatten()
        .filter(|h| !h.is_empty())
    {
        return hostname;
    }
    use rand::Rng;
    let label: String = (0..2)
        .map(|_| {
            MDNS_HOSTNAME_CHARS[rand::thread_rng().gen_range(0..MDNS_HOSTNAME_CHARS.len())] as char
        })
        .collect();
    let hostname = format!("{label}.local");
    let _ = prefs.set("mdns_hostname", &hostname);
    hostname
}

pub struct ChatDiscovery {
    browser: MdnsServiceBrowser,
    online: Arc<Mutex<HashSet<String>>>,
    hostname: Arc<RwLock<String>>,
    identity: Arc<ChatIdentity>,
    https_port: AtomicU16,
}

impl ChatDiscovery {
    /// Start the shared mDNS responder socket, install the resident
    /// browser listener and spawn the discovery worker. Publishes the
    /// service once an HTTPS port is known ([`Self::set_https_port`]).
    pub fn start(
        db: ChatDb,
        identity: Arc<ChatIdentity>,
        prefs: &crate::prefs::Prefs,
    ) -> Arc<Self> {
        let hostname = Arc::new(RwLock::new(ensure_mdns_hostname(prefs)));
        let online: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));

        // Found devices arrive on the mDNS responder packet thread — hand
        // them to a dedicated worker (mirrors plain-desktop's
        // nearby-discover-worker) so the packet thread never blocks on DB
        // access.
        let (found_tx, found_rx) = std::sync::mpsc::channel::<FoundDevice>();
        let browser = MdnsServiceBrowser::new(
            identity.client_id.clone(),
            hostname.clone(),
            move |device: FoundDevice| {
                let _ = found_tx.send(device);
            },
        );
        {
            let worker_db = db.clone();
            let worker_online = online.clone();
            let worker_id = identity.client_id.clone();
            std::thread::Builder::new()
                .name("chat-discovery-worker".into())
                .spawn(move || {
                    while let Ok(device) = found_rx.recv() {
                        if device.id == worker_id {
                            continue;
                        }
                        update_known_peer(&worker_db, &device);
                        worker_online.lock().unwrap().insert(device.id.clone());
                        let cached = plain_rs::chat::db::DNearbyDeviceCache {
                            id: device.id.clone(),
                            name: device.name.clone(),
                            ips: device.ips.clone(),
                            port: device.port,
                            device_type: device.device_type.clone(),
                            version: device.version.clone(),
                            platform: device.platform.clone(),
                            last_seen: plain_rs::chat::db::now_millis(),
                        };
                        if let Err(e) = worker_db.save_cached_nearby_device(&cached) {
                            crate::log::error!(
                                "failed to save nearby device record id={} err={e}",
                                cached.id
                            );
                        }
                    }
                })
                .expect("spawn chat-discovery-worker");
        }

        host_responder::ensure_started(&hostname.read().unwrap());
        // Seed directed queries with the paired peers' last-known
        // addresses so an offline-at-boot peer is found without a scan.
        let seeds: Vec<String> = db
            .get_peers()
            .iter()
            .filter(|p| p.is_paired() && !p.ip.is_empty())
            .flat_map(|p| {
                p.ip.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|ip| format!("{ip}:{}", p.port))
                    .collect::<Vec<_>>()
            })
            .collect();
        browser.seed_known_addrs(&seeds);
        browser.install_listener();

        Arc::new(Self {
            browser,
            online,
            hostname,
            identity,
            https_port: AtomicU16::new(0),
        })
    }

    /// Called when the HTTPS server binds (the port peers must dial).
    /// Publishes / re-publishes the `_plainapp._tcp.local` service.
    pub fn set_https_port(&self, port: u16) {
        self.https_port.store(port, Ordering::SeqCst);
        self.publish();
    }

    /// Advertise the PlainApp service on the shared responder. The
    /// instance name is the device name; TXT records carry the identity
    /// (client id / device type / version / platform).
    pub fn publish(&self) {
        let hostname = self.hostname.read().unwrap().clone();
        let port = self.https_port.load(Ordering::SeqCst);
        let device_name = self.identity.device_name();
        let service = (port > 0).then(|| {
            build_service_info(
                &device_name,
                &hostname,
                port,
                &self.identity.client_id,
                LOCAL_DEVICE_TYPE_WIRE,
                crate::version::VERSION,
                std::env::consts::OS,
                host_responder::local_ipv4_strs(),
            )
        });
        host_responder::start(&hostname, service);
    }

    /// Republish after advertised data changed (device renamed, port
    /// changed). No-op while no service is published.
    pub fn update_advertised_service(&self) {
        let port = self.https_port.load(Ordering::SeqCst);
        if port == 0 {
            return;
        }
        let hostname = self.hostname.read().unwrap().clone();
        let device_name = self.identity.device_name();
        let service = build_service_info(
            &device_name,
            &hostname,
            port,
            &self.identity.client_id,
            LOCAL_DEVICE_TYPE_WIRE,
            crate::version::VERSION,
            std::env::consts::OS,
            host_responder::local_ipv4_strs(),
        );
        host_responder::update_service(service);
    }

    pub fn is_online(&self, peer_id: &str) -> bool {
        self.online.lock().unwrap().contains(peer_id)
    }

    /// One-shot PTR re-query for directed re-discovery of a peer (used
    /// after failed delivery).
    pub fn rebrowse(&self) {
        self.browser.send_ptr_query();
    }
}

/// Refresh a known peer's address from an mDNS response — mirrors
/// plain-app `PeerManager.applyDeviceDiscovered` (bumps `updatedAt`).
/// Paired peers (chat) and logged-in peers (token) track the address;
/// unrelated peers are left untouched.
fn update_known_peer(db: &ChatDb, device: &FoundDevice) {
    let Some(mut peer) = db
        .get_peer_by_id(&device.id)
        .filter(|p| p.is_paired() || !p.token.is_empty())
    else {
        return;
    };
    let mut ips = device.ips.clone();
    ips.sort();
    let new_ip = ips.join(",");
    if new_ip != peer.ip || device.port != peer.port {
        peer.ip = new_ip;
        peer.port = device.port;
        peer.updated_at = plain_rs::chat::db::now_iso();
        db.upsert_peer(&peer);
    }
}

#[cfg(test)]
#[path = "../../tests/unit/chat/discovery.rs"]
mod tests;
