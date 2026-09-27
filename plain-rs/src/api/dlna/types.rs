use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::upper_case_acronyms)]
pub enum DlnaMediaType {
    VIDEO,
    AUDIO,
    IMAGE,
    UNKNOWN,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DlnaPlaybackState {
    NoMediaPresent,
    Stopped,
    Playing,
    PausedPlayback,
    Transitioning,
}

#[derive(Debug, Clone)]
pub enum DlnaCommand {
    SetUri {
        uri: String,
        title: String,
        media_type: DlnaMediaType,
        album_art_uri: String,
    },
    Play,
    Pause,
    Stop,
    Seek {
        position_ms: i64,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingCastRequest {
    pub sender_ip: String,
    pub sender_name: String,
    pub media_uri: String,
    pub media_title: String,
    pub media_type: DlnaMediaType,
    pub album_art_uri: String,
}
