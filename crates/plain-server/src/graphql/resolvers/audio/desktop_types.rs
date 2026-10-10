use crate::graphql::types::Instant;
use crate::graphql::types::Long;
use async_graphql::ID;
use async_graphql::SimpleObject;

/// One track of the playback queue / a playlist (plain-app `AudioItem`).
/// Stored durations are seconds; the wire field is milliseconds.
#[derive(SimpleObject, Clone, Debug)]
pub struct AudioItem {
    pub title: String,
    pub artist: String,
    pub path: String,
    #[graphql(name = "durationMs")]
    pub duration_ms: Long,
}

/// A user playlist (plain-app `AudioPlaylist`); `itemCount` is live.
#[derive(SimpleObject, Clone, Debug)]
#[graphql(complex)]
pub struct AudioPlaylist {
    pub id: ID,
    pub name: String,
    pub item_count: i32,
    #[graphql(skip)]
    pub created_at: String,
    #[graphql(skip)]
    pub updated_at: String,
}

/// Recently played track (plain-app `AudioPlayHistory`).
#[derive(SimpleObject, Clone, Debug)]
#[graphql(complex)]
pub struct AudioPlayHistory {
    pub path: String,
    pub title: String,
    pub artist: String,
    #[graphql(name = "durationMs")]
    pub duration_ms: Long,
    pub play_count: i64,
    #[graphql(skip)]
    pub played_at: String,
}

/// Audio player state (plain-app `AudioPlayback`). The desktop backend has
/// no playback engine — the queue is the shared state machine, the client
/// renders audio.
#[derive(SimpleObject)]
#[graphql(name = "AudioPlayback")]
pub struct AudioPlayback {
    pub mode: crate::assembly::enums::MediaPlayMode,
    pub current_path: Option<String>,
    pub is_playing: bool,
    pub position_ms: Long,
}

/// Audio (media index).
#[derive(SimpleObject, Clone, Debug)]
#[graphql(complex)]
pub struct Audio {
    pub id: ID,
    pub title: String,
    pub artist: String,
    pub path: String,
    #[graphql(name = "durationMs")]
    pub duration_ms: Long,
    pub size: Long,
    pub bucket_id: ID,
    pub album_file_id: String,
    pub created_at: Instant,
    pub updated_at: Instant,
    /// plain-app serves the per-track favorite flag; the NAS has no
    /// per-media favorites (folders only), so this is always false.
    pub is_favorite: bool,
}

#[async_graphql::ComplexObject]
impl AudioPlaylist {
    async fn created_at(&self) -> async_graphql::Result<crate::graphql::types::Instant> {
        crate::graphql::types::parse_instant(&self.created_at)
    }
    async fn updated_at(&self) -> async_graphql::Result<crate::graphql::types::Instant> {
        crate::graphql::types::parse_instant(&self.updated_at)
    }
}

#[async_graphql::ComplexObject]
impl AudioPlayHistory {
    async fn played_at(&self) -> async_graphql::Result<crate::graphql::types::Instant> {
        crate::graphql::types::parse_instant(&self.played_at)
    }
}
