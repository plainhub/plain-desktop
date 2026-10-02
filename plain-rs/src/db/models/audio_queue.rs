/// Play history is trimmed to this many rows once it exceeds 5/4 of it.
pub const HISTORY_KEEP: usize = 200;

/// Where the queue draws its tracks from (stored as the plain-app enum
/// name: NONE / PLAYLIST / LIBRARY).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize,
)]
pub enum QueueSourceKind {
    #[default]
    #[serde(rename = "NONE")]
    None,
    #[serde(rename = "PLAYLIST")]
    Playlist,
    #[serde(rename = "LIBRARY")]
    Library,
}

impl QueueSourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            QueueSourceKind::None => "NONE",
            QueueSourceKind::Playlist => "PLAYLIST",
            QueueSourceKind::Library => "LIBRARY",
        }
    }

    pub fn parse_stored(s: &str) -> Self {
        match s {
            "PLAYLIST" => QueueSourceKind::Playlist,
            "LIBRARY" => QueueSourceKind::Library,
            _ => QueueSourceKind::None,
        }
    }
}

/// The single queue-source row (plain-app `DAudioQueueSource`, id = 1).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QueueSource {
    pub source: QueueSourceKind,
    pub playlist_id: String,
    pub current_path: String,
    /// Position of the current track inside the source, cached for fast
    /// skips. -1 = unknown (manual jump), re-located lazily on next skip.
    pub current_index: i64,
    /// `FileSortBy` name captured when a LIBRARY source was set.
    pub sort_by: String,
}

impl Default for QueueSource {
    fn default() -> Self {
        Self {
            source: QueueSourceKind::None,
            playlist_id: String::new(),
            current_path: String::new(),
            // Matches the SQL column default: unknown position.
            current_index: -1,
            sort_by: String::new(),
        }
    }
}

/// One manual-queue row ("play next" / "add to queue").
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QueueItem {
    pub path: String,
    pub sort_order: i64,
    pub title: String,
    pub artist: String,
    pub duration_secs: i64,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PlaylistItem {
    pub id: String,
    pub playlist_id: String,
    pub audio_path: String,
    // Snapshot columns let lists render without touching the media index;
    // playback still resolves the file live from audio_path.
    pub title: String,
    pub artist: String,
    pub duration_secs: i64,
    pub sort_order: i64,
    pub added_at: String,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PlayHistory {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub duration_secs: i64,
    pub play_count: i64,
    pub played_at: String,
}
