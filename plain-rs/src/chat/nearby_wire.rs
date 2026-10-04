use super::{
    enums::DeviceType,
    pairing::protocol::{PairingCancel, PairingRequest, PairingResponse},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverReply {
    pub id: String,
    pub name: String,
    pub port: u16,
    pub device_type: DeviceType,
    pub version: String,
    pub platform: String,
    #[serde(default)]
    pub ips: Vec<String>,
    #[serde(default)]
    pub aware_supported: bool,
    #[serde(default)]
    pub aware_running: bool,
}
#[derive(Deserialize, Serialize)]
#[serde(
    tag = "kind",
    content = "payload",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum Message {
    Discover,
    DiscoverReply(DiscoverReply),
    PairRequest(PairingRequest),
    PairResponse(PairingResponse),
    PairCancel(PairingCancel),
}
impl Message {
    pub fn wire(&self) -> Result<String> {
        let value = serde_json::to_value(self)?;
        let kind = value["kind"]
            .as_str()
            .expect("serialized nearby message kind");
        Ok(format!(
            "{kind}:{}",
            value
                .get("payload")
                .map(serde_json::Value::to_string)
                .unwrap_or_default()
        ))
    }
    pub fn parse(body: &str) -> Result<Self> {
        let (kind, payload) = body
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("Missing nearby message prefix"))?;
        let mut value = serde_json::json!({"kind":kind});
        if !payload.is_empty() {
            value["payload"] = serde_json::from_str(payload)?;
        }
        Ok(serde_json::from_value(value)?)
    }
}
pub fn short_id(id: &str) -> String {
    crate::utils::hex::bytes_to_hex(&Sha256::digest(id.as_bytes())[..8])
}
pub fn discover_reply(payload: &str, advertised_id: &str) -> Result<DiscoverReply> {
    let reply: DiscoverReply = serde_json::from_str(payload)?;
    ensure!(
        short_id(&reply.id) == advertised_id,
        "BLE discovery identity mismatch"
    );
    Ok(reply)
}
#[cfg(test)]
#[path = "../../tests/unit/chat/nearby_wire.rs"]
mod tests;
