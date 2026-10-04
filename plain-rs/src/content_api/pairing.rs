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
fn request(prefs: &Prefs, device: Device) -> Result<(PairingRequest, EcdhSession)> {
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
    Ok((request, ecdh))
}
fn response(
    prefs: &Prefs,
    request: PairingRequest,
    accepted: bool,
    device: Device,
) -> Result<Option<(PairingResponse, Option<EcdhSession>)>> {
    if !security::verify_request(&request) {
        return Ok(None);
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
    Ok(Some((response, ecdh)))
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
use crate::chat::enums::DeviceType;
use crate::{
    chat::pairing::{
        peer_store::{self, Facts},
        protocol::PairingCancel,
        sessions::{Sessions, Target},
    },
    db::Db,
};
use std::str::FromStr;
pub(super) fn start(
    prefs: &Prefs,
    sessions: &Sessions,
    target: Target,
    device: Device,
) -> Result<Value> {
    if target.device_id.is_empty() {
        bail!("Missing pairing target");
    }
    let (request, ecdh) = request(prefs, device)?;
    let ticket = sessions.start(target, ecdh);
    Ok(json!({"request":request,"ticket":ticket}))
}
pub(super) fn complete(
    db: &Db,
    prefs: &Prefs,
    sessions: &Sessions,
    response: PairingResponse,
    sender_ip: &str,
) -> Result<Value> {
    if !validate_response(prefs, &response, &response.from_id)? {
        return Ok(Value::Null);
    }
    let Some(pending) = sessions.take(&response.from_id) else {
        return Ok(Value::Null);
    };
    let ticket = pending.ticket.clone();
    let result = (|| -> Result<crate::db::DPeer> {
        if pending.expired() {
            bail!("Pairing timed out");
        }
        if !response.accepted {
            bail!("Pairing request was rejected");
        }
        let key = pending
            .ecdh
            .compute_shared_key(&crate::base64_decode(&response.ecdh_public_key))
            .ok_or_else(|| anyhow::anyhow!("Failed to compute shared key"))?;
        let mut ips = vec![sender_ip.to_owned()];
        ips.extend(response.ips);
        peer_store::save(
            db,
            Facts {
                id: response.from_id,
                name: ticket.target.device_name.clone(),
                ips,
                port: response.port,
                device_type: DeviceType::from_str(&response.device_type)
                    .unwrap_or(DeviceType::Unknown),
                key: crate::base64_encode(&key),
                public_key: response.signature_public_key,
            },
        )
    })();
    Ok(match result {
        Ok(peer) => json!({"ticket":ticket,"peer":peer,"error":""}),
        Err(error) => json!({"ticket":ticket,"peer":null,"error":error.to_string()}),
    })
}
pub(super) fn respond(
    db: &Db,
    prefs: &Prefs,
    sessions: &Sessions,
    request: PairingRequest,
    accepted: bool,
    device: Device,
) -> Result<Value> {
    let Some((response, ecdh)) = response(prefs, request.clone(), accepted, device)? else {
        return Ok(Value::Null);
    };
    let Some(target) = sessions.take_incoming(&request.from_id, &request.signature) else {
        return Ok(Value::Null);
    };
    let peer = if let Some(ecdh) = ecdh {
        let key = ecdh
            .compute_shared_key(&crate::base64_decode(&request.ecdh_public_key))
            .ok_or_else(|| anyhow::anyhow!("Failed to compute shared key"))?;
        let mut ips = vec![target.device_ip];
        ips.extend(request.ips);
        Some(peer_store::save(
            db,
            Facts {
                id: request.from_id,
                name: request.from_name,
                ips,
                port: request.port,
                device_type: DeviceType::from_str(&request.device_type)
                    .unwrap_or(DeviceType::Unknown),
                key: crate::base64_encode(&key),
                public_key: request.signature_public_key,
            },
        )?)
    } else {
        None
    };
    Ok(json!({"response":response,"peer":peer}))
}
pub(super) fn cancel(
    prefs: &Prefs,
    sessions: &Sessions,
    id: &str,
    generation: Option<&str>,
) -> Result<Value> {
    let from_id = actor(prefs)?;
    Ok(sessions
        .cancel(id, generation)
        .map(|ticket| json!({"ticket":ticket,"cancel":PairingCancel{from_id,to_id:id.into()}}))
        .unwrap_or(Value::Null))
}
pub(super) fn receive_request(sessions: &Sessions, request: &PairingRequest) -> Option<bool> {
    security::verify_request(request).then(|| sessions.receive(request))
}
pub(super) fn receive_cancel(
    prefs: &Prefs,
    sessions: &Sessions,
    cancel: PairingCancel,
) -> Result<Value> {
    if cancel.to_id != actor(prefs)? || cancel.from_id.is_empty() {
        return Ok(Value::Null);
    }
    Ok(serde_json::to_value(
        sessions.receive_cancel(&cancel.from_id),
    )?)
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/pairing.rs"]
mod tests;
