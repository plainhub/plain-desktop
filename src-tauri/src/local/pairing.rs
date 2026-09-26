//! Tauri commands driving the shared pairing manager (see `local::chat`).

use plain_rs::chat::pairing::protocol::PairingRequest;

use super::chat::ChatState;

/// Initiate pairing with a discovered device.
#[tauri::command]
pub fn pair_device(
    device_id: String,
    device_name: String,
    device_ip: String,
    device_port: u16,
    state: tauri::State<'_, ChatState>,
    server_state: tauri::State<'_, super::server::LocalServerState>,
) {
    state.pairing.start_pairing(
        &device_id,
        &device_name,
        &device_ip,
        device_port,
        server_state.https_port(),
    );
}

/// Accept or reject an incoming PAIR_REQUEST.
/// `request_json` is the JSON-serialised `PairingRequest` from the `IncomingRequest` event.
#[tauri::command]
pub fn respond_pair_device(
    request_json: String,
    sender_ip: String,
    accepted: bool,
    state: tauri::State<'_, ChatState>,
    server_state: tauri::State<'_, super::server::LocalServerState>,
) -> Result<(), String> {
    let req: PairingRequest = serde_json::from_str(&request_json).map_err(|e| e.to_string())?;
    state
        .pairing
        .respond_to_pairing(req, &sender_ip, accepted, server_state.https_port());
    Ok(())
}

/// Cancel an in-progress pairing initiated by us.
#[tauri::command]
pub fn cancel_pair_device(device_id: String, state: tauri::State<'_, ChatState>) {
    state.pairing.cancel_pairing(&device_id);
}

/// Return the local device's identity (client_id, device_name, public key).
#[tauri::command]
pub fn get_device_identity(
    state: tauri::State<'_, ChatState>,
    app: tauri::AppHandle,
) -> serde_json::Value {
    let identity = &state.pairing.identity;
    let saved_name = crate::prefs::get_device_name(&app);
    let device_name = if saved_name.is_empty() {
        identity.device_name()
    } else {
        saved_name
    };
    let kp = plain_rs::base64_decode(&identity.ed25519_keypair);
    let pub_key_b64 = if kp.len() == 64 {
        plain_rs::base64_encode(&kp[32..])
    } else {
        String::new()
    };
    serde_json::json!({
        "clientId": identity.client_id,
        "deviceName": device_name,
        "publicKey": pub_key_b64,
    })
}

/// Update the local device's display name. Updates the shared name state and
/// republishes the mDNS service so peers drop the old instance and see the
/// new name right away.
#[tauri::command]
pub fn set_device_name(
    name: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, ChatState>,
    discover: tauri::State<'_, crate::commands::discover::NearbyDiscoverManager>,
) {
    crate::prefs::set_device_name(&app, &name);
    state.identity.set_device_name(&name);
    discover.apply_device_rename(&name);
}
