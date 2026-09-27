use serde::{Deserialize, Serialize};

use super::utils::device_type_signature_value;

// ── Wire-format structs (must match plain-app DNearbyPair.kt) ──────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PairingRequest {
    pub from_id: String,
    pub from_name: String,
    pub port: u16,
    pub device_type: String,
    pub ecdh_public_key: String,
    pub signature_public_key: String,
    pub timestamp: i64,
    #[serde(default)]
    pub ips: Vec<String>,
    #[serde(default)]
    pub signature: String,
    /// Whether the requester's device supports Wi-Fi Aware (mirrors
    /// plain-app `DPairingRequest.awareSupported`). Non-Aware builds
    /// always send `false`.
    #[serde(default)]
    pub aware_supported: bool,
    /// Stamped on the receiver side (mirrors plain-app
    /// `DPairingRequest.fromIp`). Carries the requester's IP so the
    /// responder knows where to send the PAIR_RESPONSE. Excluded
    /// from `signature_data` to match the Kotlin wire format.
    #[serde(default)]
    pub from_ip: String,
}

impl PairingRequest {
    pub fn signature_data(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}",
            self.from_id,
            self.from_name,
            self.port,
            device_type_signature_value(&self.device_type),
            self.ecdh_public_key,
            self.signature_public_key,
            self.timestamp,
            self.ips.join(",")
        )
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PairingResponse {
    pub from_id: String,
    pub to_id: String,
    pub port: u16,
    pub device_type: String,
    pub ecdh_public_key: String,
    pub signature_public_key: String,
    pub accepted: bool,
    pub timestamp: i64,
    #[serde(default)]
    pub ips: Vec<String>,
    #[serde(default)]
    pub signature: String,
    /// Whether the responder's device supports Wi-Fi Aware (mirrors
    /// plain-app `DPairingResponse.awareSupported`). Non-Aware builds
    /// always send `false`.
    #[serde(default)]
    pub aware_supported: bool,
}

impl PairingResponse {
    pub fn signature_data(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}",
            self.from_id,
            self.to_id,
            self.port,
            device_type_signature_value(&self.device_type),
            self.ecdh_public_key,
            self.signature_public_key,
            self.accepted,
            self.timestamp,
            self.ips.join(",")
        )
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PairingCancel {
    pub from_id: String,
    pub to_id: String,
}

#[cfg(test)]
#[path = "../../../tests/unit/chat/pairing/protocol.rs"]
mod tests;
