use std::sync::Arc;

use plain_rs::events::WsEvent;
use plain_rs::protocols::websocket as ws_frame;
use plain_server::assembly::server::ContentServer;
use tauri::ipc::{Channel, InvokeResponseBody};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::broadcast::Receiver;

#[tauri::command]
pub async fn native_events(
    core: tauri::State<'_, Arc<ContentServer>>,
    on_event: Channel<InvokeResponseBody>,
) -> Result<(), String> {
    let executor = core.native_graphql()?;
    let mut events = executor.subscribe_events();
    let cid = executor.client_id().to_owned();

    // Encrypting text payloads keeps every frame byte-identical to the local chat socket's.
    let key = plain_rs::utils::base64::base64_decode(&core.native_url_token()?);
    if key.len() != 32 {
        return Err("local event token unavailable".into());
    }

    loop {
        let Some(event) = next_event(&mut events, || executor.wait_stopped()).await else {
            return Ok(());
        };
        let Some(frame) = encode_event(&event, &cid, &key) else {
            continue;
        };
        on_event
            .send(InvokeResponseBody::Raw(frame))
            .map_err(|error| error.to_string())?;
    }
}

async fn next_event<Wait, Stopped>(
    events: &mut Receiver<WsEvent>,
    wait_stopped: Wait,
) -> Option<WsEvent>
where
    Wait: Fn() -> Stopped,
    Stopped: Future<Output = ()>,
{
    loop {
        let received = tokio::select! {
            biased;
            () = wait_stopped() => return None,
            received = events.recv() => received,
        };
        match received {
            Ok(event) => return Some(event),
            Err(RecvError::Lagged(_)) => continue,
            Err(RecvError::Closed) => return None,
        }
    }
}

fn encode_event(event: &WsEvent, cid: &str, key: &[u8]) -> Option<Vec<u8>> {
    if event.is_internal() {
        return None;
    }
    if event
        .target_cid
        .as_ref()
        .is_some_and(|target| target != cid)
    {
        return None;
    }
    match &event.binary_payload {
        Some(payload) => ws_frame::encode_raw(event.event_type, payload),
        None => ws_frame::encode(event.event_type, event.payload.as_bytes(), key),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/commands/native/events.rs"]
mod tests;
