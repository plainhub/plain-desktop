use crate::ws_event::{WS_IMAGE_EDITOR_UPDATE, WsEvent};
use std::sync::Arc;
use tokio::sync::{Semaphore, broadcast};

pub const MAX_STATE_BYTES: usize = 24 * 1024 * 1024;
const MAX_UPDATE_BYTES: usize = 8 * 1024 * 1024;

pub fn decode(value: &str) -> Result<Vec<u8>, String> {
    let value = value
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect::<String>();
    crate::utils::base64::base64_decode_checked(&value)
        .map_err(|_| "invalid image editor base64".into())
}

pub struct Updates {
    events: broadcast::Sender<WsEvent>,
    budget: Arc<Semaphore>,
}
impl Updates {
    pub fn new(events: broadcast::Sender<WsEvent>) -> Arc<Self> {
        Arc::new(Self {
            events,
            budget: Arc::new(Semaphore::new(32 * 1024 * 1024)),
        })
    }
    pub fn publish(&self, id: &str, update: &str) -> Result<(), String> {
        let id = id.as_bytes();
        if id.is_empty()
            || id.len() > u8::MAX as usize
            || update.len() > MAX_UPDATE_BYTES * 4 / 3 + 4
        {
            return Err("invalid image editor update".into());
        }
        let bytes = decode(update)?;
        if bytes.len() > MAX_UPDATE_BYTES {
            return Err("image editor update too large".into());
        }
        let size = 1 + id.len() + bytes.len();
        let permit = self
            .budget
            .clone()
            .try_acquire_many_owned(size as u32)
            .map_err(|_| "image editor update queue full")?;
        let mut payload = Vec::with_capacity(size);
        payload.push(id.len() as u8);
        payload.extend_from_slice(id);
        payload.extend_from_slice(&bytes);
        let _ = self.events.send(
            WsEvent::broadcast_binary(WS_IMAGE_EDITOR_UPDATE, payload).with_binary_permit(permit),
        );
        Ok(())
    }
}
