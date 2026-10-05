use super::{enums::DeviceType, nearby_wire::DiscoverReply};
use crate::mdns::service_info::{MdnsServiceInfo, build_service_info};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Facts {
    pub name: String,
    pub device_type: DeviceType,
    pub version: String,
    pub platform: String,
    pub ips: Vec<String>,
    pub aware_supported: bool,
    pub aware_running: bool,
}
pub fn reply(id: &str, name: &str, port: u16, facts: Facts) -> Result<DiscoverReply> {
    ensure!(!id.is_empty(), "Missing client identity");
    Ok(DiscoverReply {
        id: id.into(),
        name: if name.is_empty() {
            facts.name
        } else {
            name.into()
        },
        port,
        device_type: facts.device_type,
        version: facts.version,
        platform: facts.platform,
        ips: facts.ips,
        aware_supported: facts.aware_supported,
        aware_running: facts.aware_running,
    })
}
pub fn mdns(reply: &DiscoverReply, hostname: &str) -> Result<MdnsServiceInfo> {
    ensure!(reply.port != 0, "Missing discovery HTTPS port");
    ensure!(!hostname.is_empty(), "Missing mDNS hostname");
    Ok(build_service_info(
        &reply.name,
        hostname,
        reply.port,
        &reply.id,
        reply.device_type.as_str(),
        &reply.version,
        &reply.platform,
        reply.ips.clone(),
        reply.aware_supported,
        reply.aware_running,
    ))
}
pub fn ble(id: &str, supported: bool, running: bool) -> Result<[u8; 9]> {
    ensure!(!id.is_empty(), "Missing client identity");
    let mut payload = [0; 9];
    payload[0] = u8::from(supported) | (u8::from(running) << 1);
    payload[1..].copy_from_slice(&Sha256::digest(id.as_bytes())[..8]);
    Ok(payload)
}
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BleParts {
    pub short_id: String,
    pub aware_supported: bool,
    pub aware_running: bool,
}
pub fn decode_ble(payload: &[u8]) -> Option<BleParts> {
    if payload.len() < 9 {
        return None;
    }
    Some(BleParts {
        short_id: crate::utils::hex::bytes_to_hex(&payload[1..9]),
        aware_supported: payload[0] & 1 != 0,
        aware_running: payload[0] & 2 != 0,
    })
}
#[cfg(test)]
#[path = "../../tests/unit/chat/discovery_advertisement.rs"]
mod tests;
