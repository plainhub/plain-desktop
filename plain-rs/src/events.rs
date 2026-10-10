#[derive(Clone, Debug)]
pub struct WsEvent {
    host_origin: bool,
    binary_permit: Option<std::sync::Arc<tokio::sync::OwnedSemaphorePermit>>,
    pub event_type: &'static str,
    pub payload: String,
    pub binary_payload: Option<Vec<u8>>,
    /// Delivery target: `Some(cid)` delivers only to the socket whose cid
    /// matches (per-client media/file-task/DLNA progress); `None`
    /// broadcasts to every connected socket.
    pub target_cid: Option<String>,
}

pub const HOST_TEXT_TYPES: &[&str] = &[
    "SCREEN_MIRRORING",
    "SCREEN_MIRROR_AUDIO_GRANTED",
    "SCREEN_MIRROR_VIDEO_CODEC",
    "SMS_PROVIDER_CHANGED",
    "SMS_SEND_RESULT",
    "MMS_SEND_RESULT",
    "CLIPBOARD_CHANGED",
    "PERMISSIONS_UPDATED",
];
pub const HOST_BINARY_TYPES: &[&str] = &["SCREEN_MIRROR_VIDEO", "SCREEN_MIRROR_AUDIO"];

impl WsEvent {
    pub fn is_internal(&self) -> bool {
        matches!(
            self.event_type,
            "MDNS_UPDATED"
                | "PEER_CONNECTIONS_UPDATED"
                | "PEER_TRANSPORT_UPDATED"
                | "SHARED_FOLDER_DOWNLOAD_UPDATED"
                | "DLNA_RENDERER_UPDATED"
                | "ONLINE_CLIENTS_UPDATED"
                | "WEB_REQUEST_RECEIVED"
                | "DLNA_SENDER_UPDATED"
                | "IMAGE_MODELS_UPDATED"
                | "PREFS_UPDATED"
                | "NEARBY_DEVICES_UPDATED"
        )
    }

    pub fn is_host_event(&self) -> bool {
        self.host_origin
    }

    pub fn host_text(event_type: &str, payload: String) -> Option<Self> {
        let event_type = *HOST_TEXT_TYPES.iter().find(|&&kind| kind == event_type)?;
        let mut event = Self::broadcast(event_type, payload);
        event.host_origin = true;
        Some(event)
    }

    pub fn host_binary(event_type: &str, payload: Vec<u8>) -> Option<Self> {
        let event_type = *HOST_BINARY_TYPES.iter().find(|&&kind| kind == event_type)?;
        let mut event = Self::broadcast_binary(event_type, payload);
        event.host_origin = true;
        Some(event)
    }

    pub fn with_binary_permit(mut self, permit: tokio::sync::OwnedSemaphorePermit) -> Self {
        self.binary_permit = Some(std::sync::Arc::new(permit));
        self
    }
    pub fn broadcast(event_type: &'static str, payload: String) -> Self {
        Self {
            host_origin: false,
            binary_permit: None,
            event_type,
            payload,
            binary_payload: None,
            target_cid: None,
        }
    }

    pub fn targeted(event_type: &'static str, payload: String, cid: &str) -> Self {
        Self {
            host_origin: false,
            binary_permit: None,
            event_type,
            payload,
            binary_payload: None,
            target_cid: Some(cid.to_string()),
        }
    }

    pub fn broadcast_binary(event_type: &'static str, payload: Vec<u8>) -> Self {
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

pub const WS_BOOKMARK_UPDATED: &'static str = "BOOKMARK_UPDATED";

pub const WS_POMODORO_ACTION: &'static str = "POMODORO_ACTION";

pub const WS_IMAGE_EDITOR_UPDATE: &'static str = "IMAGE_EDITOR_UPDATE";
