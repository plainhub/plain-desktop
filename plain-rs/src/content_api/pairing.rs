use super::peer_wire::signing_keypair;
use crate::{
    chat::pairing::{
        now_ms,
        protocol::{PairingRequest, PairingResponse},
        security,
    },
    crypto::EcdhSession,
    prefs::Prefs,
};
use anyhow::{Result, bail};
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
pub(super) struct Device {
    pub name: String,
    pub port: u16,
    pub device_type: String,
    pub ips: Vec<String>,
    pub aware_supported: bool,
}
fn actor(prefs: &Prefs) -> Result<String> {
    prefs
        .get::<String>("client_id")?
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Missing pairing identity"))
}
pub(super) fn request(prefs: &Prefs, device: Device) -> Result<Value> {
    let kp = signing_keypair(prefs)?;
    let ecdh = EcdhSession::generate();
    let mut request = PairingRequest {
        from_id: actor(prefs)?,
        from_name: device.name,
        port: device.port,
        device_type: device.device_type,
        ecdh_public_key: crate::base64_encode(&ecdh.public_key_bytes),
        signature_public_key: crate::base64_encode(&kp[32..]),
        timestamp: now_ms(),
        ips: device.ips,
        signature: String::new(),
        aware_supported: device.aware_supported,
        from_ip: String::new(),
    };
    request.signature = crate::ed25519_sign(&kp, request.signature_data().as_bytes());
    if !security::verify_request(&request) {
        bail!("Invalid pairing signing identity");
    }
    Ok(
        json!({"request":request,"privateKey":crate::base64_encode(&ecdh.private_key_bytes()),"publicKey":crate::base64_encode(&ecdh.public_key_bytes)}),
    )
}
pub(super) fn response(
    prefs: &Prefs,
    request: PairingRequest,
    accepted: bool,
    device: Device,
) -> Result<Value> {
    if !security::verify_request(&request) {
        return Ok(Value::Null);
    }
    let kp = signing_keypair(prefs)?;
    let ecdh = accepted.then(EcdhSession::generate);
    let mut response = PairingResponse {
        from_id: actor(prefs)?,
        to_id: request.from_id,
        port: device.port,
        device_type: device.device_type,
        ecdh_public_key: ecdh
            .as_ref()
            .map(|e| crate::base64_encode(&e.public_key_bytes))
            .unwrap_or_default(),
        signature_public_key: crate::base64_encode(&kp[32..]),
        accepted,
        timestamp: now_ms(),
        ips: device.ips,
        signature: String::new(),
        aware_supported: device.aware_supported,
    };
    response.signature = crate::ed25519_sign(&kp, response.signature_data().as_bytes());
    if !security::verify_response(&response, &response.to_id, &response.from_id) {
        bail!("Invalid pairing signing identity");
    }
    Ok(
        json!({"response":response,"privateKey":ecdh.map(|e|crate::base64_encode(&e.private_key_bytes()))}),
    )
}
pub(super) fn validate_response(
    prefs: &Prefs,
    response: &PairingResponse,
    expected: &str,
) -> Result<bool> {
    Ok(security::verify_response(
        response,
        &actor(prefs)?,
        expected,
    ))
}
pub(super) fn derive(private: &str, public: &str) -> Option<String> {
    EcdhSession::from_private_key(&crate::base64_decode(private))?
        .compute_shared_key(&crate::base64_decode(public))
        .map(|k| crate::base64_encode(&k))
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/pairing.rs"]
mod tests;
