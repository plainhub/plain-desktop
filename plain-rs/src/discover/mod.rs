//! Nearby-device discovery stack: LAN service discovery (mDNS browser +
//! responder), peer online/offline state, the macOS dns_sd FFI browser,
//! and Windows mDNS firewall diagnostics.

pub mod firewall;
pub mod macos_dns_sd;
pub mod nearby_discover_manager;
pub mod peer_status_manager;

pub use firewall::MdnsFirewallStatus;
pub use nearby_discover_manager::NearbyDiscoverManager;
pub use peer_status_manager::PeerStatusManager;

/// One mDNS debug-activity entry (mirrors plain-app's MdnsDebugPage feed).
#[derive(Clone, Debug, serde::Serialize)]
pub struct MdnsActivity {
    pub time: u64,
    pub event: String,
    pub detail: String,
}
