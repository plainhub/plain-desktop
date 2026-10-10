//! Media-library resolution seam over the media stack, shared by every
//! host that runs the audio queue (plain-nas and the desktop local API):
//! implements `plain_rs::media::audio::queue::LibraryTracks` on top of
//! the tantivy index + fjall media rows, and resolves a single track
//! from a path. Ported from plain-nas's `NasLibraryTracks`.
//!
//! Storage and behavior (queue ordering, supersede, playlists, history)
//! all live in `plain_rs::media`; only what touches the media index lives
//! here.

use std::sync::Arc;

use plain_rs::error::DomainError;
use plain_rs::error::DomainResult;
use plain_rs::media::audio::queue::AudioTrack;
use plain_rs::media::audio::queue::LibraryTracks;

use crate::index::images::MediaSearchIndex;
use crate::index::images::MediaSearchResult;
use crate::index::images::MediaSort;
use crate::storage::kv::Db;

/// tantivy search returns `anyhow::Error`; flatten to a message.
fn anyhow_free(e: anyhow::Error) -> DomainError {
    DomainError::Other(e.to_string())
}

/// `FileSortBy` name (stored on the source row) → media index sort.
/// Unknown names degrade to DATE_DESC — the phone's
/// `AudioSortByPreference` default.
fn library_sort_of(sort_by: &str) -> MediaSort {
    match sort_by {
        "DATE_ASC" => MediaSort::DateAsc,
        "SIZE_ASC" => MediaSort::SizeAsc,
        "SIZE_DESC" => MediaSort::SizeDesc,
        "NAME_ASC" => MediaSort::NameAsc,
        "NAME_DESC" => MediaSort::NameDesc,
        _ => MediaSort::DateDesc,
    }
}

fn search_result_to_audio(r: MediaSearchResult) -> AudioTrack {
    AudioTrack {
        title: if r.title.is_empty() {
            r.name.clone()
        } else {
            r.title
        },
        artist: r.artist,
        path: r.path,
        duration_ms: (r.duration_secs as i64).saturating_mul(1000),
        album_id: String::new(),
    }
}

/// Library-source resolution over the media index: paging via the
/// tantivy fast-field order, metadata hydration (probe once, persist)
/// from the fjall media rows.
pub struct MediaLibraryTracks {
    db: Arc<Db>,
    index: Arc<MediaSearchIndex>,
}

impl MediaLibraryTracks {
    pub fn new(db: Arc<Db>, index: Arc<MediaSearchIndex>) -> Self {
        Self { db, index }
    }
}

impl LibraryTracks for MediaLibraryTracks {
    fn library_count(&mut self) -> DomainResult<usize> {
        Ok(self
            .index
            .count("", Some("audio"), None)
            .map_err(anyhow_free)?)
    }

    fn library_path_at(&mut self, offset: usize, sort_by: &str) -> DomainResult<Option<String>> {
        Ok(self
            .index
            .search("", Some("audio"), None, library_sort_of(sort_by), offset, 1)
            .map_err(anyhow_free)?
            .into_iter()
            .next()
            .map(|r| r.path))
    }

    fn library_tracks_page(
        &mut self,
        offset: usize,
        limit: usize,
        sort_by: &str,
    ) -> DomainResult<Vec<AudioTrack>> {
        let mut hits = self
            .index
            .search(
                "",
                Some("audio"),
                None,
                library_sort_of(sort_by),
                offset,
                limit,
            )
            .map_err(anyhow_free)?;
        crate::scan::hydrate_search_page(&self.db, &mut hits, true);
        Ok(hits.into_iter().map(search_result_to_audio).collect())
    }

    fn library_locate(&mut self, path: &str, sort_by: &str) -> DomainResult<i64> {
        let size = self.library_count()?;
        let sort = library_sort_of(sort_by);
        // Paged scan (cursor re-location only).
        let page_size = 500;
        let mut offset = 0;
        while offset < size {
            let page = self
                .index
                .search("", Some("audio"), None, sort, offset, page_size)
                .map_err(anyhow_free)?;
            if page.is_empty() {
                break;
            }
            if let Some(hit) = page.iter().position(|r| r.path == path) {
                return Ok((offset + hit) as i64);
            }
            offset += page.len();
        }
        Ok(-1)
    }

    fn library_contains(&mut self, path: &str) -> DomainResult<bool> {
        Ok(crate::scan::get_by_path(&self.db, path)
            .map(|m| m.is_some())
            .unwrap_or(false))
    }
}

/// Resolve a track for queue/playlist insertion: metadata comes from the
/// media index row when indexed, otherwise the tags are probed from the
/// file itself (the phone reads them via MediaMetadataRetriever). The
/// title falls back to the file name without extension.
pub fn playlist_audio_from_path(db: &Db, path: &str) -> AudioTrack {
    let path = path.replace('\\', "/");
    let fallback_title = std::path::Path::new(&path)
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or(&path)
        .to_string();
    if let Ok(Some(mut m)) = crate::scan::get_by_path(db, &path) {
        // Same hydration the Go playlist builder does before building the
        // track: fill missing tags/duration once, persisted with the row.
        crate::scan::hydrate_metadata(db, &mut m);
        return AudioTrack {
            title: if m.title.is_empty() {
                fallback_title
            } else {
                m.title
            },
            artist: m.artist,
            duration_ms: (m.duration_sec as i64).saturating_mul(1000),
            album_id: String::new(),
            path,
        };
    }
    // Unindexed file: one combined probe, best effort.
    let m = crate::metadata::probe_media(&path, "audio");
    AudioTrack {
        title: if m.title.is_empty() {
            fallback_title
        } else {
            m.title
        },
        artist: m.artist,
        duration_ms: (m.duration_secs as i64).saturating_mul(1000),
        album_id: String::new(),
        path,
    }
}
