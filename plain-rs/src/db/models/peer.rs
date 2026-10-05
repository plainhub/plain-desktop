use crate::chat::enums::{DeviceType, PeerStatus};
use crate::db::now_iso;
use crate::utils::http_url::build_url;

/// Matches plain-app `DPeer` entity.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DPeer {
    pub id: String,
    pub name: String,
    pub ip: String,
    /// Base64-encoded XChaCha20 shared key (ECDH-derived). Empty until paired.
    pub key: String,
    /// Base64-encoded raw Ed25519 public key (32 bytes).
    pub public_key: String,
    pub status: PeerStatus,
    pub port: u16,
    pub device_type: DeviceType,
    /// Login session token (XChaCha20 shared key, base64). Empty when logged out.
    pub token: String,
    pub created_at: String,
    pub updated_at: String,
}

impl DPeer {
    pub fn new(id: &str, name: &str, ip: &str, port: u16, device_type: DeviceType) -> Self {
        let now = now_iso();
        Self {
            id: id.to_string(),
            name: name.to_string(),
            ip: ip.to_string(),
            key: String::new(),
            public_key: String::new(),
            status: PeerStatus::Unpaired,
            port,
            device_type,
            token: String::new(),
            created_at: now.clone(),
            updated_at: now,
        }
    }

    pub fn is_paired(&self) -> bool {
        self.status == PeerStatus::Paired
    }

    pub fn best_ip(&self) -> String {
        let ips = self
            .ip
            .split(',')
            .map(str::trim)
            .filter(|ip| !ip.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        crate::chat::lan_ip::best(&ips, &crate::chat::lan_ip::local_interfaces())
    }

    pub fn base_url(&self) -> String {
        build_url("https", &self.best_ip(), self.port, "")
    }

    pub fn peer_graphql_url(&self) -> String {
        format!("{}/peer_graphql", self.base_url())
    }

    pub fn file_url(&self, file_id: &str) -> String {
        format!("{}/fs?id={}", self.base_url(), file_id)
    }
}
