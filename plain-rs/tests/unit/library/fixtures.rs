//! Shared test fixtures for the library modules — a temp [`LibraryDb`]
//! per test and a deterministic in-memory [`FakeLibrary`] standing in
//! for a platform media index.
//!
//! Included (`#[path]`) by several test modules; not every consumer uses
//! every item.
#![allow(dead_code)]

use crate::library::LibraryResult;
use crate::library::audio_queue::AudioTrack;
use crate::library::audio_queue::LibraryTracks;
use crate::library::db::LibraryDb;
use std::sync::atomic::{AtomicUsize, Ordering};

static SEQ: AtomicUsize = AtomicUsize::new(0);

/// Private per-test SQLite file (unique dir per tag + seq + pid): full
/// isolation from parallel tests and previous runs.
pub fn test_db(tag: &str) -> LibraryDb {
    let n = SEQ.fetch_add(1, Ordering::SeqCst);
    let p = std::env::temp_dir().join(format!("plain-rs-library-{tag}-{n}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    LibraryDb::open(&p.join("library.db")).unwrap()
}

/// Deterministic library for the LIBRARY-source tests: tracks in the
/// given order under every sort (sort_by is ignored), `contains` is a
/// path lookup. No I/O, no wall clock.
pub struct FakeLibrary {
    pub tracks: Vec<AudioTrack>,
}

impl FakeLibrary {
    pub fn new(paths: &[(&str, i64)]) -> Self {
        Self {
            tracks: paths
                .iter()
                .map(|&(p, d)| AudioTrack {
                    title: format!("T-{}", p.rsplit('/').next().unwrap_or(p)),
                    artist: "A".to_string(),
                    path: p.to_string(),
                    duration_secs: d,
                })
                .collect(),
        }
    }
}

impl LibraryTracks for FakeLibrary {
    fn library_count(&mut self) -> LibraryResult<usize> {
        Ok(self.tracks.len())
    }
    fn library_path_at(&mut self, offset: usize, _sort_by: &str) -> LibraryResult<Option<String>> {
        Ok(self.tracks.get(offset).map(|t| t.path.clone()))
    }
    fn library_tracks_page(
        &mut self,
        offset: usize,
        limit: usize,
        _sort_by: &str,
    ) -> LibraryResult<Vec<AudioTrack>> {
        Ok(self
            .tracks
            .iter()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect())
    }
    fn library_locate(&mut self, path: &str, _sort_by: &str) -> LibraryResult<i64> {
        Ok(self
            .tracks
            .iter()
            .position(|t| t.path == path)
            .map(|p| p as i64)
            .unwrap_or(-1))
    }
    fn library_contains(&mut self, path: &str) -> LibraryResult<bool> {
        Ok(self.tracks.iter().any(|t| t.path == path))
    }
}

pub fn audio(path: &str, title: &str, duration_secs: i64) -> AudioTrack {
    AudioTrack {
        title: title.to_string(),
        artist: "A".to_string(),
        path: path.to_string(),
        duration_secs,
    }
}

pub fn paths_of(items: &[AudioTrack]) -> Vec<String> {
    items.iter().map(|a| a.path.clone()).collect()
}
