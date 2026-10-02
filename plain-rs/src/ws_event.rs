#[derive(Clone, Debug)]
pub struct WsEvent {
    binary_permit: Option<std::sync::Arc<tokio::sync::OwnedSemaphorePermit>>,
    pub event_type: i32,
    pub payload: String,
    pub binary_payload: Option<Vec<u8>>,
    /// Delivery target: `Some(cid)` delivers only to the socket whose cid
    /// matches (per-client media/file-task/DLNA progress); `None`
    /// broadcasts to every connected socket.
    pub target_cid: Option<String>,
}

impl WsEvent {
    pub fn with_binary_permit(mut self, permit: tokio::sync::OwnedSemaphorePermit) -> Self {
        self.binary_permit = Some(std::sync::Arc::new(permit));
        self
    }
    pub fn broadcast(event_type: i32, payload: String) -> Self {
        Self {
            binary_permit: None,
            event_type,
            payload,
            binary_payload: None,
            target_cid: None,
        }
    }

    pub fn targeted(event_type: i32, payload: String, cid: &str) -> Self {
        Self {
            binary_permit: None,
            event_type,
            payload,
            binary_payload: None,
            target_cid: Some(cid.to_string()),
        }
    }

    pub fn broadcast_binary(event_type: i32, payload: Vec<u8>) -> Self {
        Self {
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
