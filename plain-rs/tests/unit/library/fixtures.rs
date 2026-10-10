//! Shared test fixtures for the library modules — a temp [`Db`]
//! per test and a deterministic in-memory [`FakeLibrary`] standing in
//! for a platform media index.
//!
//! Included (`#[path]`) by several test modules; not every consumer uses
//! every item. Paths name `plain_rs::` explicitly so the same file also
//! works when plain-media mounts it from its own crate root.
#![allow(dead_code)]

use plain_rs::db::Db;
use plain_rs::error::DomainResult;
use plain_rs::media::audio::queue::AudioTrack;
use plain_rs::media::audio::queue::LibraryTracks;

/// Private per-test SQLite file. The dir comes from `tempfile`, so
/// concurrent tests never collide — this file is `#[path]`-mounted
/// several times in one binary and a hand-rolled counter would restart
/// per mount.
pub fn test_db(tag: &str) -> Db {
    let dir = tempfile::Builder::new()
        .prefix(&format!("plain-rs-library-{tag}-"))
        .tempdir()
        .unwrap();
    let db = Db::open(&dir.path().join("plain.db")).unwrap();
    plain_rs::test_tempdirs::retain(dir);
    db
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
                    duration_ms: d,
                    album_id: String::new(),
                })
                .collect(),
        }
    }
}

impl LibraryTracks for FakeLibrary {
    fn library_count(&mut self) -> DomainResult<usize> {
        Ok(self.tracks.len())
    }
    fn library_path_at(&mut self, offset: usize, _sort_by: &str) -> DomainResult<Option<String>> {
        Ok(self.tracks.get(offset).map(|t| t.path.clone()))
    }
    fn library_tracks_page(
        &mut self,
        offset: usize,
        limit: usize,
        _sort_by: &str,
    ) -> DomainResult<Vec<AudioTrack>> {
        Ok(self
            .tracks
            .iter()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect())
    }
    fn library_locate(&mut self, path: &str, _sort_by: &str) -> DomainResult<i64> {
        Ok(self
            .tracks
            .iter()
            .position(|t| t.path == path)
            .map(|p| p as i64)
            .unwrap_or(-1))
    }
    fn library_contains(&mut self, path: &str) -> DomainResult<bool> {
        Ok(self.tracks.iter().any(|t| t.path == path))
    }
}

pub fn audio(path: &str, title: &str, duration_ms: i64) -> AudioTrack {
    AudioTrack {
        title: title.to_string(),
        artist: "A".to_string(),
        path: path.to_string(),
        duration_ms,
        album_id: String::new(),
    }
}

pub fn paths_of(items: &[AudioTrack]) -> Vec<String> {
    items.iter().map(|a| a.path.clone()).collect()
}
