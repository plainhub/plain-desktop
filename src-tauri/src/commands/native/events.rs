use std::sync::Arc;

use plain_rs::protocols::websocket as ws_frame;
use plain_server::assembly::server::ContentServer;
use tauri::ipc::{Channel, InvokeResponseBody};
use tokio::sync::broadcast::error::RecvError;

#[tauri::command]
pub async fn native_events(
    core: tauri::State<'_, Arc<ContentServer>>,
    prefs: tauri::State<'_, Arc<crate::prefs::Prefs>>,
    on_event: Channel<InvokeResponseBody>,
) -> Result<(), String> {
    let executor = core.native_graphql()?;
    let mut events = executor.subscribe_events();
    let cid = executor.client_id().to_owned();

    // Encrypting text payloads keeps every frame byte-identical to the local chat socket's.
    let key =
        plain_rs::utils::base64::base64_decode(&crate::prefs::ensure_url_token(prefs.inner()));
    if key.len() != 32 {
        return Err("local event token unavailable".into());
    }

    loop {
        let event = match events.recv().await {
            Ok(event) => event,
            Err(RecvError::Lagged(_)) => continue,
            Err(RecvError::Closed) => return Ok(()),
        };
        if event.is_internal() {
            continue;
        }
        if event
            .target_cid
            .as_ref()
            .is_some_and(|target| target != &cid)
        {
            continue;
        }
        let frame = match &event.binary_payload {
            Some(payload) => ws_frame::encode_raw(event.event_type, payload),
            None => ws_frame::encode(event.event_type, event.payload.as_bytes(), &key),
        };
        let Some(frame) = frame else {
            continue;
        };
        on_event
            .send(InvokeResponseBody::Raw(frame))
            .map_err(|error| error.to_string())?;
    }
}
