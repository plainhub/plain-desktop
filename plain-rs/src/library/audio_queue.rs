//! Audio playback queue, user playlists and play history — the shared
//! behavior port of plain-app `AudioQueueManager` + its Room tables.
//!
//! The queue is never a materialized list. It is:
//!
//! - a **source** — a user playlist or the whole library — plus
//! - a small **manual queue** ("play next" / "add to queue" items), and
//! - the **current** track.
//!
//! The next/previous track is resolved from the source on demand with indexed
//! queries, so playing a 10k-item library costs the same as playing one item.
//!
//! Playback order (ranks 0..total-1):
//!
//! ```text
//! source[0 .. currentPos]  ->  manual queue  ->  source[currentPos+1 ..]
//! ```
//!
//! Manually queued tracks play right after the current one, then the source
//! continues where it left off. Source copies of manually queued tracks are
//! *superseded* — the manual slot is the one that plays — which is what keeps
//! the rendered queue free of duplicates.
//!
//! Library resolution (the LIBRARY source) is platform-specific: NAS serves
//! it from its media search index, desktop has no media index yet. The
//! [`LibraryTracks`] trait is that seam; everything above it — ordering,
//! supersede, paging, history, playlist CRUD — is identical on both ends.

use std::collections::HashSet;

use crate::library::LibraryResult;
use crate::library::db::{
    HISTORY_KEEP, LibraryDb, PlayHistory, Playlist, PlaylistItem, QueueItem, QueueSource,
    QueueSourceKind,
};
use crate::utils::dbtime::now_iso_millis;
use crate::utils::shortid;

/// The track shape served by queue/playlist APIs (`PlaylistAudio`).
#[derive(Clone, Debug, PartialEq)]
pub struct AudioTrack {
    pub title: String,
    pub artist: String,
    pub path: String,
    pub duration_secs: i64,
}

impl AudioTrack {
    /// Minimal metadata for a track no index knows about: the title is
    /// the file name without extension, everything else empty/zero.
    pub fn from_path_stem(path: &str) -> Self {
        let path = path.replace('\\', "/");
        let title = std::path::Path::new(&path)
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or(&path)
            .to_string();
        Self {
            title,
            artist: String::new(),
            path,
            duration_secs: 0,
        }
    }
}

impl From<&QueueItem> for AudioTrack {
    fn from(q: &QueueItem) -> Self {
        Self {
            title: q.title.clone(),
            artist: q.artist.clone(),
            path: q.path.clone(),
            duration_secs: q.duration_secs,
        }
    }
}

impl From<&PlaylistItem> for AudioTrack {
    fn from(i: &PlaylistItem) -> Self {
        Self {
            title: i.title.clone(),
            artist: i.artist.clone(),
            path: i.audio_path.clone(),
            duration_secs: i.duration_secs,
        }
    }
}

/// Library-source resolution seam. NAS implements it over the tantivy
/// media index (+ metadata hydration); consumers without a media index
/// use [`NoLibrary`].
///
/// `sort_by` is the plain-app `FileSortBy` name captured on the source
/// row ("DATE_DESC", "NAME_ASC", …); implementors map unknown names to
/// their default (the phone default is DATE_DESC).
pub trait LibraryTracks {
    /// Total audio tracks in the library.
    fn library_count(&mut self) -> LibraryResult<usize>;
    /// Path identity at `offset` under `sort_by` — a pure index lookup,
    /// never probes files (used for cached-position validation).
    fn library_path_at(&mut self, offset: usize, sort_by: &str) -> LibraryResult<Option<String>>;
    /// A page of tracks starting at `offset` under `sort_by`, hydrated
    /// with the metadata the player shows (probing + persisting is the
    /// implementor's job).
    fn library_tracks_page(
        &mut self,
        offset: usize,
        limit: usize,
        sort_by: &str,
    ) -> LibraryResult<Vec<AudioTrack>>;
    /// Index of `path` in the library under `sort_by`, -1 when absent.
    fn library_locate(&mut self, path: &str, sort_by: &str) -> LibraryResult<i64>;
    /// Whether `path` exists in the library index (supersede checks).
    fn library_contains(&mut self, path: &str) -> LibraryResult<bool>;
}

/// Empty library — consumers with no media index (desktop today). The
/// LIBRARY source resolves to zero tracks; playlists and the manual
/// queue work unchanged.
pub struct NoLibrary;

impl LibraryTracks for NoLibrary {
    fn library_count(&mut self) -> LibraryResult<usize> {
        Ok(0)
    }
    fn library_path_at(&mut self, _offset: usize, _sort_by: &str) -> LibraryResult<Option<String>> {
        Ok(None)
    }
    fn library_tracks_page(
        &mut self,
        _offset: usize,
        _limit: usize,
        _sort_by: &str,
    ) -> LibraryResult<Vec<AudioTrack>> {
        Ok(vec![])
    }
    fn library_locate(&mut self, _path: &str, _sort_by: &str) -> LibraryResult<i64> {
        Ok(-1)
    }
    fn library_contains(&mut self, _path: &str) -> LibraryResult<bool> {
        Ok(false)
    }
}

/// Play-mode preference key (plain-app `AudioPlayModePreference`).
const PREF_AUDIO_MODE: &str = "audio_play_mode";
const DEFAULT_AUDIO_MODE: &str = "REPEAT";

// ---------------------------------------------------------------------------
// Current source / current track / play mode
// ---------------------------------------------------------------------------

pub fn source(db: &LibraryDb) -> QueueSource {
    crate::library::db::audio_queue::get_source(db)
}

pub fn save_source(db: &LibraryDb, src: &QueueSource) {
    crate::library::db::audio_queue::save_source(db, src)
}

/// The path of the current track — `App.audioCurrent` serves this.
pub fn get_audio_current(db: &LibraryDb) -> String {
    source(db).current_path
}

/// Overwrite the current track on the source row.
pub fn save_audio_current(db: &LibraryDb, path: &str) {
    let mut src = source(db);
    src.current_path = path.to_string();
    save_source(db, &src);
}

/// The play mode name (REPEAT / REPEAT_ONE / SHUFFLE); REPEAT when unset.
pub fn get_audio_mode(db: &LibraryDb) -> String {
    let raw = crate::library::db::audio_queue::get_pref(db, PREF_AUDIO_MODE).unwrap_or_default();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        DEFAULT_AUDIO_MODE.to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn save_audio_mode(db: &LibraryDb, mode: &str) {
    // Store trimmed — a clean preference value.
    crate::library::db::audio_queue::set_pref(db, PREF_AUDIO_MODE, mode.trim());
}

/// Playlist id when the active playback source is a user playlist, else None.
pub fn active_playlist_id(db: &LibraryDb) -> Option<String> {
    let src = source(db);
    (src.source == QueueSourceKind::Playlist).then_some(src.playlist_id)
}

/// Record that `path` started playing (manual jumps included). Port of
/// plain-app `AudioQueueManager.onPlaying` — a manual jump breaks the cached
/// library position; it is marked unknown and re-located lazily on the next
/// sequential skip.
pub fn on_playing(db: &LibraryDb, path: &str, title: &str, artist: &str, duration_secs: i64) {
    if path.is_empty() {
        return;
    }
    let mut src = source(db);
    let new_index = if src.source == QueueSourceKind::Library && path != src.current_path {
        -1
    } else {
        src.current_index
    };
    if src.current_path != path || new_index != src.current_index {
        src.current_path = path.to_string();
        src.current_index = new_index;
        save_source(db, &src);
    }
    record_history(db, path, title, artist, duration_secs);
}

// ---------------------------------------------------------------------------
// Manual queue
// ---------------------------------------------------------------------------

/// Add tracks to the manual queue. `play_next` moves/inserts them at the
/// front. Existing entries for the same path are moved, never duplicated.
pub fn enqueue(db: &LibraryDb, items: &[AudioTrack], play_next: bool) {
    if items.is_empty() {
        return;
    }
    let mut queued = crate::library::db::audio_queue::all_queue_items(db);
    let incoming: Vec<&str> = items.iter().map(|a| a.path.as_str()).collect();
    queued.retain(|q| !incoming.contains(&q.path.as_str()));
    let rows: Vec<QueueItem> = items
        .iter()
        .map(|a| QueueItem {
            path: a.path.clone(),
            sort_order: 0,
            title: a.title.clone(),
            artist: a.artist.clone(),
            duration_secs: a.duration_secs,
        })
        .collect();
    if play_next {
        let mut out = rows;
        out.extend(queued);
        queued = out;
    } else {
        queued.extend(rows);
    }
    // replace_queue_items assigns dense sort orders.
    crate::library::db::audio_queue::replace_queue_items(db, &queued);
}

pub fn remove_queued(db: &LibraryDb, path: &str) {
    crate::library::db::audio_queue::remove_queue_item(db, path)
}

/// Reorder the manual queue to match `paths`; unknown paths keep their
/// order at the end.
pub fn reorder_queued(db: &LibraryDb, paths: &[String]) {
    if paths.is_empty() {
        return;
    }
    let all = crate::library::db::audio_queue::all_queue_items(db);
    if all.is_empty() {
        return;
    }
    let known: HashSet<&str> = paths.iter().map(|s| s.as_str()).collect();
    let mut ordered: Vec<QueueItem> = Vec::with_capacity(all.len());
    for p in paths {
        if let Some(item) = all.iter().find(|i| i.path == *p) {
            ordered.push(item.clone());
        }
    }
    for item in &all {
        if !known.contains(item.path.as_str()) {
            ordered.push(item.clone());
        }
    }
    crate::library::db::audio_queue::replace_queue_items(db, &ordered);
}

/// Cascade cleanup when media files are deleted or trashed.
pub fn remove_paths(db: &LibraryDb, paths: &[String]) {
    if paths.is_empty() {
        return;
    }
    crate::library::db::audio_queue::remove_by_paths(
        db,
        "DELETE FROM audio_queue_items WHERE path",
        paths,
    );
    crate::library::db::audio_queue::remove_history(db, paths);
    crate::library::db::audio_queue::remove_playlist_items_by_paths(db, paths);
    let mut src = source(db);
    if !src.current_path.is_empty() && paths.contains(&src.current_path) {
        src.current_path = String::new();
        src.current_index = -1;
        save_source(db, &src);
    }
}

/// Reset the source and the manual queue. Stopping playback is the caller's
/// job (`clearAudioQueue` also clears the current track).
pub fn clear_queue(db: &LibraryDb) {
    crate::library::db::audio_queue::replace_queue_items(db, &[]);
    save_source(db, &QueueSource::default());
}

// ---------------------------------------------------------------------------
// Playback order
// ---------------------------------------------------------------------------

struct Order {
    src: QueueSource,
    manual_count: usize,
    source_size: usize,
    /// Position of the current track inside the source, -1 if not in it.
    current_pos: i64,
}

impl Order {
    fn total(&self) -> usize {
        self.manual_count + self.source_size
    }
}

fn source_size_of(
    db: &LibraryDb,
    lib: &mut dyn LibraryTracks,
    src: &QueueSource,
) -> LibraryResult<usize> {
    match src.source {
        QueueSourceKind::Playlist => {
            Ok(crate::library::db::audio_queue::playlist_items(db, &src.playlist_id).len())
        }
        QueueSourceKind::Library => lib.library_count(),
        QueueSourceKind::None => Ok(0),
    }
}

fn current_pos_in_source(
    db: &LibraryDb,
    lib: &mut dyn LibraryTracks,
    src: &mut QueueSource,
    source_size: usize,
) -> LibraryResult<i64> {
    let path = src.current_path.clone();
    if path.is_empty() || source_size == 0 {
        return Ok(-1);
    }
    match src.source {
        QueueSourceKind::Playlist => Ok(crate::library::db::audio_queue::playlist_items(
            db,
            &src.playlist_id,
        )
        .into_iter()
        .position(|i| i.audio_path == path)
        .map(|p| p as i64)
        .unwrap_or(-1)),
        QueueSourceKind::Library => {
            let sort = src.sort_by.clone();
            let cached = src.current_index;
            if cached >= 0
                && (cached as usize) < source_size
                && lib
                    .library_path_at(cached as usize, &sort)?
                    .is_some_and(|p| p == path)
            {
                return Ok(cached);
            }
            let found = lib.library_locate(&path, &sort)?;
            if found >= 0 {
                src.current_index = found;
                save_source(db, src);
            }
            Ok(found)
        }
        QueueSourceKind::None => Ok(-1),
    }
}

fn playback_order(db: &LibraryDb, lib: &mut dyn LibraryTracks) -> LibraryResult<Order> {
    let mut src = source(db);
    let manual_count = crate::library::db::audio_queue::all_queue_items(db).len();
    let source_size = source_size_of(db, lib, &src)?;
    let current_pos = current_pos_in_source(db, lib, &mut src, source_size)?;
    Ok(Order {
        src,
        manual_count,
        source_size,
        current_pos,
    })
}

/// Rank of the current track in the playback order, -1 if unknown.
fn current_rank(order: &Order, queued: &[QueueItem]) -> i64 {
    let path = order.src.current_path.as_str();
    if path.is_empty() {
        return -1;
    }
    if let Some(pos) = queued.iter().position(|q| q.path == path) {
        // Queued items start right after the head: currentPos+1 + rank in queue.
        return order.current_pos + 1 + pos as i64;
    }
    // The current track is in the source and always closes the head segment.
    order.current_pos
}

fn track_at(
    db: &LibraryDb,
    lib: &mut dyn LibraryTracks,
    order: &Order,
    queued: &[QueueItem],
    rank: i64,
) -> LibraryResult<Option<AudioTrack>> {
    if rank < 0 || rank >= order.total() as i64 {
        return Ok(None);
    }
    if rank <= order.current_pos {
        source_track_at(db, lib, &order.src, rank)
    } else if rank <= order.current_pos + order.manual_count as i64 {
        let qrank = (rank - order.current_pos - 1) as usize;
        Ok(queued.get(qrank).map(AudioTrack::from))
    } else {
        source_track_at(db, lib, &order.src, rank - order.manual_count as i64)
    }
}

fn source_track_at(
    db: &LibraryDb,
    lib: &mut dyn LibraryTracks,
    src: &QueueSource,
    rank: i64,
) -> LibraryResult<Option<AudioTrack>> {
    match src.source {
        QueueSourceKind::Playlist => Ok(crate::library::db::audio_queue::playlist_items(
            db,
            &src.playlist_id,
        )
        .into_iter()
        .nth(rank.max(0) as usize)
        .map(|i| AudioTrack::from(&i))),
        QueueSourceKind::Library => Ok(lib
            .library_tracks_page(rank.max(0) as usize, 1, &src.sort_by)?
            .into_iter()
            .next()),
        QueueSourceKind::None => Ok(None),
    }
}

/// Source copies of manually queued tracks are superseded: the manual slot
/// is the one that plays, so they are skipped in total/count, rendering and
/// sequential resolution. This is what keeps the queue free of duplicates.
fn superseded_source_paths(
    db: &LibraryDb,
    lib: &mut dyn LibraryTracks,
    order: &Order,
    queued: &[QueueItem],
) -> LibraryResult<HashSet<String>> {
    if order.manual_count == 0 || order.src.source == QueueSourceKind::None {
        return Ok(HashSet::new());
    }
    let queued_set: HashSet<&str> = queued.iter().map(|q| q.path.as_str()).collect();
    match order.src.source {
        QueueSourceKind::Playlist => Ok(crate::library::db::audio_queue::playlist_items(
            db,
            &order.src.playlist_id,
        )
        .into_iter()
        .filter(|i| queued_set.contains(i.audio_path.as_str()))
        .map(|i| i.audio_path)
        .collect()),
        QueueSourceKind::Library => {
            // The phone resolves `ids:<queued>` through the media index; a
            // point lookup per queued path is the same answer here.
            let mut out = HashSet::new();
            for q in queued {
                if lib.library_contains(&q.path)? {
                    out.insert(q.path.clone());
                }
            }
            Ok(out)
        }
        QueueSourceKind::None => Ok(HashSet::new()),
    }
}

fn save_current(
    db: &LibraryDb,
    lib: &mut dyn LibraryTracks,
    order: &Order,
    queued: &[QueueItem],
    rank: i64,
) -> LibraryResult<()> {
    let path = match track_at(db, lib, order, queued, rank)? {
        Some(a) => a.path,
        None => return Ok(()),
    };
    let source_pos: i64 = match order.src.source {
        QueueSourceKind::Playlist => {
            crate::library::db::audio_queue::playlist_items(db, &order.src.playlist_id)
                .into_iter()
                .position(|i| i.audio_path == path)
                .map(|p| p as i64)
                .unwrap_or(-1)
        }
        QueueSourceKind::Library => {
            if order.current_pos >= 0 && rank <= order.current_pos {
                rank
            } else if order.current_pos >= 0
                && rank <= order.current_pos + order.manual_count as i64
            {
                -1
            } else {
                rank - order.manual_count as i64
            }
        }
        QueueSourceKind::None => -1,
    };
    let mut src = order.src.clone();
    src.current_path = path;
    src.current_index = source_pos;
    save_source(db, &src);
    Ok(())
}

// ---------------------------------------------------------------------------
// Next / previous resolution
// ---------------------------------------------------------------------------

/// Resolve the next/previous track in the playback order and advance the
/// current track. Returns None when there is nothing to play.
pub fn resolve_next(
    db: &LibraryDb,
    lib: &mut dyn LibraryTracks,
    is_next: bool,
    shuffle: bool,
) -> LibraryResult<Option<AudioTrack>> {
    let order = playback_order(db, lib)?;
    if order.total() == 0 {
        return Ok(None);
    }
    let queued = crate::library::db::audio_queue::all_queue_items(db);
    let superseded = superseded_source_paths(db, lib, &order, &queued)?;
    let current = current_rank(&order, &queued);
    let total = (order.total() - superseded.len()) as i64;
    if total == 0 {
        return Ok(None);
    }
    let mut target: i64 = if shuffle {
        use rand::Rng;
        rand::thread_rng().gen_range(0..total)
    } else {
        let from = if current < 0 {
            // No current track: "next" starts from before the first rank,
            // "previous" from before the last one.
            if is_next { -1 } else { 0 }
        } else {
            current
        };
        if is_next {
            (from + 1) % total
        } else {
            (from - 1 + total) % total
        }
    };
    // Walk past source copies of manually queued tracks — they play from
    // their manual slot instead and must not repeat.
    let mut audio = track_at(db, lib, &order, &queued, target)?;
    while let Some(a) = &audio {
        if !superseded.contains(&a.path) {
            break;
        }
        target = if is_next {
            (target + 1) % order.total() as i64
        } else {
            (target - 1 + order.total() as i64) % order.total() as i64
        };
        let next = track_at(db, lib, &order, &queued, target)?;
        if next.as_ref().map(|n| n.path == a.path).unwrap_or(false) {
            break;
        }
        audio = next;
    }
    let audio = match audio {
        Some(a) if !superseded.contains(&a.path) => a,
        _ => return Ok(None),
    };
    save_current(db, lib, &order, &queued, target)?;
    record_history(
        db,
        &audio.path,
        &audio.title,
        &audio.artist,
        audio.duration_secs,
    );
    Ok(Some(audio))
}

// ---------------------------------------------------------------------------
// Queue totals / paging
// ---------------------------------------------------------------------------

pub fn queue_total(db: &LibraryDb, lib: &mut dyn LibraryTracks) -> LibraryResult<usize> {
    let order = playback_order(db, lib)?;
    let queued = crate::library::db::audio_queue::all_queue_items(db);
    let superseded = superseded_source_paths(db, lib, &order, &queued)?;
    Ok(order.total() - superseded.len())
}

/// A page of the playback order — never materializes the whole queue. When
/// `text` is set (the DSL `text:` field, case-insensitive substring over
/// title/artist/path) the order is paged through in chunks and only the
/// matching tracks are kept, so filtering precedes pagination.
pub fn queue_page(
    db: &LibraryDb,
    lib: &mut dyn LibraryTracks,
    offset: i64,
    limit: i64,
    text: &str,
) -> LibraryResult<Vec<AudioTrack>> {
    let needle = text.trim().to_lowercase();
    if needle.is_empty() {
        return queue_page_unfiltered(db, lib, offset, limit);
    }
    let want = (offset.max(0) + limit.max(0)) as usize;
    const CHUNK: i64 = 500;
    let mut matched: Vec<AudioTrack> = Vec::new();
    let mut rank = 0i64;
    while matched.len() < want {
        let page = queue_page_unfiltered(db, lib, rank, CHUNK)?;
        if page.is_empty() {
            break;
        }
        rank += page.len() as i64;
        matched.extend(page.into_iter().filter(|a| audio_matches_text(a, &needle)));
    }
    Ok(matched
        .into_iter()
        .skip(offset.max(0) as usize)
        .take(limit.max(0) as usize)
        .collect())
}

/// Case-insensitive substring match over the fields a track renders.
fn audio_matches_text(a: &AudioTrack, needle: &str) -> bool {
    a.title.to_lowercase().contains(needle)
        || a.artist.to_lowercase().contains(needle)
        || a.path.to_lowercase().contains(needle)
}

fn queue_page_unfiltered(
    db: &LibraryDb,
    lib: &mut dyn LibraryTracks,
    offset: i64,
    limit: i64,
) -> LibraryResult<Vec<AudioTrack>> {
    let order = playback_order(db, lib)?;
    let queued = crate::library::db::audio_queue::all_queue_items(db);
    let superseded = superseded_source_paths(db, lib, &order, &queued)?;
    let mut out: Vec<AudioTrack> = Vec::new();
    let mut rank = offset.max(0);
    let end = (offset + limit).min(order.total() as i64);
    while rank < end {
        if rank <= order.current_pos {
            // head: source up to the current track
            let seg_end = end.min(order.current_pos + 1);
            let rows = source_page(db, lib, &order.src, rank, seg_end - rank)?;
            out.extend(rows.into_iter().filter(|a| !superseded.contains(&a.path)));
            rank = seg_end;
        } else if rank <= order.current_pos + order.manual_count as i64 {
            // manual queue
            let seg_end = end.min(order.current_pos + 1 + order.manual_count as i64);
            let from = (rank - order.current_pos - 1) as usize;
            let take = (seg_end - rank) as usize;
            out.extend(queued.iter().skip(from).take(take).map(AudioTrack::from));
            rank = seg_end;
        } else {
            // tail: the rest of the source
            let rows = source_page(
                db,
                lib,
                &order.src,
                rank - order.manual_count as i64,
                end - rank,
            )?;
            out.extend(rows.into_iter().filter(|a| !superseded.contains(&a.path)));
            rank = end;
        }
    }
    Ok(out)
}

fn source_page(
    db: &LibraryDb,
    lib: &mut dyn LibraryTracks,
    src: &QueueSource,
    offset: i64,
    limit: i64,
) -> LibraryResult<Vec<AudioTrack>> {
    if limit <= 0 {
        return Ok(vec![]);
    }
    match src.source {
        QueueSourceKind::Playlist => Ok(crate::library::db::audio_queue::playlist_items(
            db,
            &src.playlist_id,
        )
        .into_iter()
        .skip(offset.max(0) as usize)
        .take(limit as usize)
        .map(|i| AudioTrack::from(&i))
        .collect()),
        QueueSourceKind::Library => {
            lib.library_tracks_page(offset.max(0) as usize, limit as usize, &src.sort_by)
        }
        QueueSourceKind::None => Ok(vec![]),
    }
}

// ---------------------------------------------------------------------------
// Set playback source
// ---------------------------------------------------------------------------

/// Play a user playlist: make it the source, clear the manual queue.
/// Returns the track to start with.
pub fn set_playlist_source(
    db: &LibraryDb,
    playlist_id: &str,
    start_path: Option<&str>,
) -> Option<AudioTrack> {
    crate::library::db::audio_queue::replace_queue_items(db, &[]);
    let items = crate::library::db::audio_queue::playlist_items(db, playlist_id);
    if items.is_empty() {
        save_source(db, &QueueSource::default());
        return None;
    }
    let start = match start_path {
        Some(p) => items
            .iter()
            .find(|i| i.audio_path == p)
            .unwrap_or(&items[0]),
        None => &items[0],
    };
    let src = QueueSource {
        source: QueueSourceKind::Playlist,
        playlist_id: playlist_id.to_string(),
        current_path: start.audio_path.clone(),
        current_index: start.sort_order,
        ..QueueSource::default()
    };
    save_source(db, &src);
    let track = AudioTrack::from(start);
    record_history(
        db,
        &track.path,
        &track.title,
        &track.artist,
        track.duration_secs,
    );
    Some(track)
}

/// Play the whole library: make it the source, clear the manual queue.
/// Returns the track to start with.
pub fn set_library_source(
    db: &LibraryDb,
    lib: &mut dyn LibraryTracks,
    start_path: Option<&str>,
    shuffle: bool,
) -> LibraryResult<Option<AudioTrack>> {
    crate::library::db::audio_queue::replace_queue_items(db, &[]);
    let size = lib.library_count()?;
    if size == 0 {
        save_source(db, &QueueSource::default());
        return Ok(None);
    }
    // plain-app sorts the library source by the AudioSortByPreference
    // default (DATE_DESC).
    let sort = "DATE_DESC";
    let mut start_index: i64 = 0;
    let start = if shuffle {
        use rand::Rng;
        start_index = rand::thread_rng().gen_range(0..size) as i64;
        lib.library_tracks_page(start_index as usize, 1, sort)?
            .into_iter()
            .next()
    } else if let Some(p) = start_path {
        start_index = lib.library_locate(p, sort)?;
        if start_index >= 0 {
            lib.library_tracks_page(start_index as usize, 1, sort)?
                .into_iter()
                .next()
        } else {
            None
        }
    } else {
        lib.library_tracks_page(0, 1, sort)?.into_iter().next()
    };
    let start = match start {
        Some(s) => s,
        None => {
            save_source(db, &QueueSource::default());
            return Ok(None);
        }
    };
    let src = QueueSource {
        source: QueueSourceKind::Library,
        current_path: start.path.clone(),
        current_index: start_index,
        sort_by: sort.to_string(),
        ..QueueSource::default()
    };
    save_source(db, &src);
    record_history(
        db,
        &start.path,
        &start.title,
        &start.artist,
        start.duration_secs,
    );
    Ok(Some(start))
}

// ---------------------------------------------------------------------------
// User playlists
// ---------------------------------------------------------------------------

pub fn playlists(db: &LibraryDb) -> Vec<(Playlist, usize)> {
    let all = crate::library::db::audio_queue::all_playlists(db);
    let counts = crate::library::db::audio_queue::playlist_item_counts(db);
    all.into_iter()
        .map(|pl| {
            let count = counts.get(&pl.id).copied().unwrap_or(0);
            (pl, count)
        })
        .collect()
}

pub fn playlist_by_id(db: &LibraryDb, id: &str) -> Option<Playlist> {
    crate::library::db::audio_queue::playlist_by_id(db, id)
}

pub fn create_playlist(db: &LibraryDb, name: &str) -> Playlist {
    let now = now_iso_millis();
    let pl = Playlist {
        id: shortid::new_id(),
        name: name.to_string(),
        created_at: now.clone(),
        updated_at: now,
    };
    crate::library::db::audio_queue::insert_playlist(db, &pl);
    pl
}

pub fn rename_playlist(db: &LibraryDb, id: &str, name: &str) {
    if let Some(mut pl) = crate::library::db::audio_queue::playlist_by_id(db, id) {
        pl.name = name.to_string();
        pl.updated_at = now_iso_millis();
        crate::library::db::audio_queue::update_playlist(db, &pl);
    }
}

pub fn delete_playlist(db: &LibraryDb, id: &str) {
    crate::library::db::audio_queue::delete_playlist(db, id);
    crate::library::db::audio_queue::delete_playlist_items(db, id);
    let mut src = source(db);
    if src.source == QueueSourceKind::Playlist && src.playlist_id == id {
        src.source = QueueSourceKind::None;
        src.playlist_id = String::new();
        save_source(db, &src);
    }
}

/// Add tracks to a playlist; duplicates (same path) are ignored.
/// Returns how many were added.
pub fn add_playlist_items(db: &LibraryDb, playlist_id: &str, items: &[AudioTrack]) -> usize {
    let mut existing = crate::library::db::audio_queue::playlist_items(db, playlist_id);
    let mut next = existing.last().map(|i| i.sort_order + 1).unwrap_or(0);
    let mut added = 0;
    let now = now_iso_millis();
    for a in items {
        if existing.iter().any(|i| i.audio_path == a.path) {
            continue;
        }
        let row = PlaylistItem {
            id: shortid::new_id(),
            playlist_id: playlist_id.to_string(),
            audio_path: a.path.clone(),
            title: a.title.clone(),
            artist: a.artist.clone(),
            duration_secs: a.duration_secs,
            sort_order: next,
            added_at: now.clone(),
        };
        next += 1;
        added += 1;
        crate::library::db::audio_queue::insert_playlist_item(db, &row);
        existing.push(row);
    }
    touch_playlist(db, playlist_id);
    added
}

fn touch_playlist(db: &LibraryDb, id: &str) {
    if let Some(mut pl) = crate::library::db::audio_queue::playlist_by_id(db, id) {
        pl.updated_at = now_iso_millis();
        crate::library::db::audio_queue::update_playlist(db, &pl);
    }
}

pub fn remove_playlist_item(db: &LibraryDb, playlist_id: &str, path: &str) {
    crate::library::db::audio_queue::remove_playlist_item(db, playlist_id, path);
    touch_playlist(db, playlist_id);
}

pub fn playlist_items_page(
    db: &LibraryDb,
    playlist_id: &str,
    offset: i64,
    limit: i64,
    text: &str,
) -> Vec<AudioTrack> {
    let needle = text.trim().to_lowercase();
    crate::library::db::audio_queue::playlist_items(db, playlist_id)
        .into_iter()
        .filter(|i| {
            needle.is_empty()
                || i.title.to_lowercase().contains(&needle)
                || i.artist.to_lowercase().contains(&needle)
                || i.audio_path.to_lowercase().contains(&needle)
        })
        .skip(offset.max(0) as usize)
        .take(limit.max(0) as usize)
        .map(|i| AudioTrack::from(&i))
        .collect()
}

pub fn playlist_item_count(db: &LibraryDb, playlist_id: &str) -> usize {
    crate::library::db::audio_queue::playlist_items(db, playlist_id).len()
}

// ---------------------------------------------------------------------------
// Play history
// ---------------------------------------------------------------------------

fn record_history(db: &LibraryDb, path: &str, title: &str, artist: &str, duration_secs: i64) {
    let existing = crate::library::db::audio_queue::history_by_path(db, path);
    let row = match existing {
        Some(mut h) => {
            h.play_count += 1;
            h.played_at = now_iso_millis();
            h.title = title.to_string();
            h.artist = artist.to_string();
            h.duration_secs = duration_secs;
            h
        }
        None => PlayHistory {
            path: path.to_string(),
            title: title.to_string(),
            artist: artist.to_string(),
            duration_secs,
            play_count: 1,
            played_at: now_iso_millis(),
        },
    };
    crate::library::db::audio_queue::upsert_history(db, &row);
    let len = crate::library::db::audio_queue::all_history(db).len();
    if len > HISTORY_KEEP * 5 / 4 {
        crate::library::db::audio_queue::trim_history(db, HISTORY_KEEP);
    }
}

/// Recently played tracks, newest first, `text` filtering before paging.
pub fn history_page(db: &LibraryDb, offset: i64, limit: i64, text: &str) -> Vec<PlayHistory> {
    let needle = text.trim().to_lowercase();
    crate::library::db::audio_queue::all_history(db)
        .into_iter()
        .filter(|h| {
            needle.is_empty()
                || h.title.to_lowercase().contains(&needle)
                || h.artist.to_lowercase().contains(&needle)
                || h.path.to_lowercase().contains(&needle)
        })
        .skip(offset.max(0) as usize)
        .take(limit.max(0) as usize)
        .collect()
}

#[cfg(test)]
#[path = "../../tests/unit/library/audio_queue.rs"]
mod tests;
