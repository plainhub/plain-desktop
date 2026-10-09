#[derive(Clone, Debug)]
pub struct WsEvent {
    host_origin: bool,
    binary_permit: Option<std::sync::Arc<tokio::sync::OwnedSemaphorePermit>>,
    pub event_type: i32,
    pub payload: String,
    pub binary_payload: Option<Vec<u8>>,
    /// Delivery target: `Some(cid)` delivers only to the socket whose cid
    /// matches (per-client media/file-task/DLNA progress); `None`
    /// broadcasts to every connected socket.
    pub target_cid: Option<String>,
}

pub const HOST_TEXT_TYPES: &[i32] = &[5, 14, 32, 35, 36, 37, 39, 40];
pub const HOST_BINARY_TYPES: &[i32] = &[31, 33];

impl WsEvent {
    pub fn is_host_event(&self) -> bool {
        self.host_origin
    }

    pub fn host_text(event_type: i32, payload: String) -> Option<Self> {
        if !HOST_TEXT_TYPES.contains(&event_type) {
            return None;
        }
        let mut event = Self::broadcast(event_type, payload);
        event.host_origin = true;
        Some(event)
    }

    pub fn host_binary(event_type: i32, payload: Vec<u8>) -> Option<Self> {
        if !HOST_BINARY_TYPES.contains(&event_type) {
            return None;
        }
        let mut event = Self::broadcast_binary(event_type, payload);
        event.host_origin = true;
        Some(event)
    }

    pub fn with_binary_permit(mut self, permit: tokio::sync::OwnedSemaphorePermit) -> Self {
        self.binary_permit = Some(std::sync::Arc::new(permit));
        self
    }
    pub fn broadcast(event_type: i32, payload: String) -> Self {
        Self {
            host_origin: false,
            binary_permit: None,
            event_type,
            payload,
            binary_payload: None,
            target_cid: None,
        }
    }

    pub fn targeted(event_type: i32, payload: String, cid: &str) -> Self {
        Self {
            host_origin: false,
            binary_permit: None,
            event_type,
            payload,
            binary_payload: None,
            target_cid: Some(cid.to_string()),
        }
    }

    pub fn broadcast_binary(event_type: i32, payload: Vec<u8>) -> Self {
        Self {
            host_origin: false,
            binary_permit: None,
            event_type,
            payload: String::new(),
            binary_payload: Some(payload),
            target_cid: None,
        }
    }
}

pub const WS_BOOKMARK_UPDATED: i32 = 15;

pub const WS_POMODORO_ACTION: i32 = 11;

pub const WS_IMAGE_EDITOR_UPDATE: i32 = 34;
