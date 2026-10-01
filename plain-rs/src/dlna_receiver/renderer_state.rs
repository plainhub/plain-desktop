use crate::dlna_receiver::types::{DlnaMediaType, DlnaPlaybackState, PendingCastRequest};

#[derive(Clone)]
pub struct DlnaRendererState {
    pub is_running: bool,
    pub is_retrying: bool,
    pub media_uri: String,
    pub media_title: String,
    pub media_album_art_uri: String,
    pub media_type: DlnaMediaType,
    pub playback_state: DlnaPlaybackState,
    pub port: u16,
    pub current_position_ms: i64,
    pub duration_ms: i64,
    pub seek_target_ms: Option<i64>,
    pub raw_pending_cast_request: Option<PendingCastRequest>,
    pub pending_cast_request: Option<PendingCastRequest>,
    pub pending_play_queued: bool,
    pub start_error: String,
}

impl Default for DlnaRendererState {
    fn default() -> Self {
        Self {
            is_running: false,
            is_retrying: false,
            media_uri: String::new(),
            media_title: String::new(),
            media_album_art_uri: String::new(),
            media_type: DlnaMediaType::UNKNOWN,
            playback_state: DlnaPlaybackState::NoMediaPresent,
            port: 0,
            current_position_ms: 0,
            duration_ms: 0,
            seek_target_ms: None,
            raw_pending_cast_request: None,
            pending_cast_request: None,
            pending_play_queued: false,
            start_error: String::new(),
        }
    }
}

impl DlnaRendererState {
    pub fn reset(&mut self) {
        self.media_uri.clear();
        self.media_title.clear();
        self.media_album_art_uri.clear();
        self.media_type = DlnaMediaType::UNKNOWN;
        self.playback_state = DlnaPlaybackState::NoMediaPresent;
        self.current_position_ms = 0;
        self.duration_ms = 0;
        self.seek_target_ms = None;
        self.raw_pending_cast_request = None;
        self.pending_cast_request = None;
        self.pending_play_queued = false;
        self.start_error.clear();
    }
}
