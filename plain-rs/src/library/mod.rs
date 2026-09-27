//! Shared user-library domain — SQLite-backed audio playback queue /
//! playlists / play history, tags + tag relations and favorite folders.
//!
//! Ported 1:1 from plain-nas's fjall implementations (which were ports of
//! plain-app's Room tables) so NAS and desktop run identical behavior:
//! both consumers call these functions against a [`db::LibraryDb`]
//! SQLite file; only the media-library resolution seam
//! ([`audio_queue::LibraryTracks`]) is implemented per platform.

pub mod audio_queue;
pub mod db;
pub mod favorite_folders;
pub mod tags;

/// Library-domain error: a SQLite failure or a domain-level message
/// (e.g. "tag not found"). `Send + Sync` so it converts into anyhow and
/// GraphQL error types at the consumers' boundaries.
#[derive(Debug)]
pub enum LibraryError {
    Sqlite(rusqlite::Error),
    Other(String),
}

impl std::fmt::Display for LibraryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LibraryError::Sqlite(e) => write!(f, "sqlite: {e}"),
            LibraryError::Other(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for LibraryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LibraryError::Sqlite(e) => Some(e),
            LibraryError::Other(_) => None,
        }
    }
}

impl From<rusqlite::Error> for LibraryError {
    fn from(e: rusqlite::Error) -> Self {
        LibraryError::Sqlite(e)
    }
}

pub type LibraryResult<T> = std::result::Result<T, LibraryError>;
