use crate::services::peers::peer_wire::signing_keypair;
use anyhow::{Result, bail};
use plain_rs::crypto::EcdhSession;
use plain_rs::peers::pairing::now_ms;
use plain_rs::peers::pairing::protocol::PairingRequest;
use plain_rs::peers::pairing::protocol::PairingResponse;
use plain_rs::peers::pairing::security;
use plain_rs::prefs::Prefs;
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
pub(crate) struct Device {
    pub name: String,
    pub port: u16,
    pub device_type: String,
    pub ips: Vec<String>,
    pub aware_supported: bool,
}
fn actor(prefs: &Prefs) -> Result<String> {
    prefs
        .get::<String>(plain_rs::prefs::system::keys::CLIENT_ID)?
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
        ecdh_public_key: plain_rs::utils::base64::base64_encode(&ecdh.public_key_bytes),
        signature_public_key: plain_rs::utils::base64::base64_encode(&kp[32..]),
        timestamp: now_ms(),
        ips: device.ips,
        signature: String::new(),
        aware_supported: device.aware_supported,
        from_ip: String::new(),
    };
    request.signature = plain_rs::crypto::ed25519_sign(&kp, request.signature_data().as_bytes());
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
            .map(|e| plain_rs::utils::base64::base64_encode(&e.public_key_bytes))
            .unwrap_or_default(),
        signature_public_key: plain_rs::utils::base64::base64_encode(&kp[32..]),
        accepted,
        timestamp: now_ms(),
        ips: device.ips,
        signature: String::new(),
        aware_supported: device.aware_supported,
    };
    response.signature = plain_rs::crypto::ed25519_sign(&kp, response.signature_data().as_bytes());
    if !security::verify_response(&response, &response.to_id, &response.from_id) {
        bail!("Invalid pairing signing identity");
    }
    Ok(Some((response, ecdh)))
}
pub(crate) fn validate_response(
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
use plain_rs::chat::enums::DeviceType;
use plain_rs::db::Db;
use plain_rs::peers::pairing::peer_store;
use plain_rs::peers::pairing::peer_store::Facts;
use plain_rs::peers::pairing::protocol::PairingCancel;
use plain_rs::peers::pairing::sessions::Sessions;
use plain_rs::peers::pairing::sessions::Target;
use std::str::FromStr;
pub(crate) fn start(
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
pub(crate) fn complete(
    db: &Db,
    prefs: &Prefs,
    sessions: &Sessions,
    response: PairingResponse,
    sender_ip: &str,
) -> Result<Value> {
    complete_checked(db, prefs, sessions, response, sender_ip, None)
}
pub(crate) fn complete_checked(
    db: &Db,
    prefs: &Prefs,
    sessions: &Sessions,
    response: PairingResponse,
    sender_ip: &str,
    generation: Option<&str>,
) -> Result<Value> {
    if !validate_response(prefs, &response, &response.from_id)? {
        return Ok(Value::Null);
    }
    let Some(pending) = sessions.take_checked(&response.from_id, generation) else {
        return Ok(Value::Null);
    };
    let ticket = pending.ticket.clone();
    let result = (|| -> Result<plain_rs::db::DPeer> {
        if pending.expired() {
            bail!("Pairing timed out");
        }
        if !response.accepted {
            bail!("Pairing request was rejected");
        }
        let key = pending
            .ecdh
            .compute_shared_key(&plain_rs::utils::base64::base64_decode(
                &response.ecdh_public_key,
            ))
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
                key: plain_rs::utils::base64::base64_encode(&key),
                public_key: response.signature_public_key,
            },
        )
    })();
    Ok(match result {
        Ok(peer) => {
            json!({"ticket":ticket,"peer":crate::services::peers::peer_address::view(&peer),"error":""})
        }
        Err(error) => json!({"ticket":ticket,"peer":null,"error":error.to_string()}),
    })
}
pub(crate) fn respond(
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
            .compute_shared_key(&plain_rs::utils::base64::base64_decode(
                &request.ecdh_public_key,
            ))
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
                key: plain_rs::utils::base64::base64_encode(&key),
                public_key: request.signature_public_key,
            },
        )?)
    } else {
        None
    };
    Ok(
        json!({"response":response,"peer":peer.as_ref().map(crate::services::peers::peer_address::view)}),
    )
}
pub(crate) fn cancel(
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
pub(crate) fn receive_request(sessions: &Sessions, request: &PairingRequest) -> Option<bool> {
    security::verify_request(request).then(|| sessions.receive(request))
}
pub(crate) fn receive_cancel(
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
#[path = "../../../tests/unit/services/discovery/pairing.rs"]
mod tests;
