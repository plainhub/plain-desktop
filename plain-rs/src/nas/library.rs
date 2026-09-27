//! NAS seam over the shared `crate::library` core: opens
//! `library.db` under the data dir and re-exports the media-library
//! resolution (`MediaLibraryTracks`) that plain-rs implements on top of
//! the tantivy index + fjall media rows.
//!
//! Storage and behavior (queue ordering, supersede, playlists, history,
//! tags, favorite folders) all live in plain-rs; the audio-queue
//! GraphQL resolvers here assemble the tracks source with the NAS's fjall
//! handle + media index.

pub use crate::media::library_tracks::{
    MediaLibraryTracks as NasLibraryTracks, playlist_audio_from_path,
};
