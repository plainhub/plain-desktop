use plain_rs::peers::pairing::{protocol::PairingRequest, sessions::Target};
use plain_server::assembly::server::ContentServer;
use plain_server::native::pairing::DeviceIdentity;
use std::sync::Arc;

#[tauri::command]
pub async fn pair_device(
    device_id: String,
    device_name: String,
    device_ip: String,
    device_port: u16,
    core: tauri::State<'_, Arc<ContentServer>>,
) -> Result<(), String> {
    core.native_pairing()?
        .start(
            Target {
                device_id,
                device_name,
                device_ip: device_ip.clone(),
                device_port,
            },
            vec![device_ip],
            vec!["LAN".into()],
        )
        .await
}

#[tauri::command]
pub async fn respond_pair_device(
    request_json: String,
    sender_ip: String,
    accepted: bool,
    core: tauri::State<'_, Arc<ContentServer>>,
) -> Result<(), String> {
    let request: PairingRequest =
        serde_json::from_str(&request_json).map_err(|error| error.to_string())?;
    core.native_pairing()?
        .respond(request, &sender_ip, accepted)
        .await
}

#[tauri::command]
pub async fn cancel_pair_device(
    device_id: String,
    core: tauri::State<'_, Arc<ContentServer>>,
) -> Result<(), String> {
    core.native_pairing()?.cancel(&device_id).await
}

#[tauri::command]
pub fn get_device_identity(
    core: tauri::State<'_, Arc<ContentServer>>,
) -> Result<DeviceIdentity, String> {
    core.native_pairing()?.identity()
}

#[tauri::command]
pub async fn set_device_name(
    name: String,
    core: tauri::State<'_, Arc<ContentServer>>,
) -> Result<bool, String> {
    core.native_pairing()?.set_device_name(name).await
}
