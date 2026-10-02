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

use crate::db::audio_queue::io;
use crate::library::LibraryError;
use std::collections::HashSet;

use crate::db::{
    Db, HISTORY_KEEP, PlayHistory, Playlist, PlaylistItem, QueueItem, QueueSource, QueueSourceKind,
};
use crate::library::LibraryResult;
use crate::utils::dbtime::now_iso_millis;
use crate::utils::shortid;

/// The track shape served by queue/playlist APIs (`PlaylistAudio`).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioTrack {
    pub title: String,
    pub artist: String,
    pub path: String,
    pub duration_ms: i64,
    pub album_id: String,
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
            duration_ms: 0,
            album_id: String::new(),
        }
    }
}

impl From<&QueueItem> for AudioTrack {
    fn from(q: &QueueItem) -> Self {
        Self {
            title: q.title.clone(),
            artist: q.artist.clone(),
            path: q.path.clone(),
            duration_ms: q.duration_ms,
            album_id: String::new(),
        }
    }
}

impl From<&PlaylistItem> for AudioTrack {
    fn from(i: &PlaylistItem) -> Self {
        Self {
            title: i.title.clone(),
            artist: i.artist.clone(),
            path: i.audio_path.clone(),
            duration_ms: i.duration_ms,
            album_id: i.album_id.clone(),
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

pub fn source(db: &Db) -> LibraryResult<QueueSource> {
    Ok(crate::db::audio_queue::get_source(db)?)
}

pub fn save_source(db: &Db, src: &QueueSource) -> LibraryResult<()> {
    crate::db::audio_queue::save_source(db, src)?;
    Ok(())
}

/// The path of the current track — `App.audioCurrent` serves this.
pub fn get_audio_current(db: &Db) -> LibraryResult<String> {
    Ok(source(db)?.current_path)
}

/// Overwrite the current track on the source row.
pub fn save_audio_current(db: &Db, path: &str) -> LibraryResult<()> {
    transaction(db, |c| {
        let mut src = io::get_source(c)?;
        src.current_path = path.into();
        io::save_source(c, &src)?;
        Ok(())
    })
}

/// The play mode name (REPEAT / REPEAT_ONE / SHUFFLE); REPEAT when unset.
pub fn get_audio_mode(prefs: &crate::prefs::Prefs) -> LibraryResult<String> {
    let raw = prefs
        .get_user::<String>(PREF_AUDIO_MODE)
        .map_err(|e| LibraryError::Other(e.to_string()))?
        .unwrap_or_else(|| DEFAULT_AUDIO_MODE.into());
    let trimmed = raw.trim();
    Ok(if trimmed.is_empty() {
        DEFAULT_AUDIO_MODE.into()
    } else {
        trimmed.into()
    })
}

pub fn save_audio_mode(prefs: &crate::prefs::Prefs, mode: &str) -> crate::prefs::Result<bool> {
    // Store trimmed — a clean preference value.
    prefs.set_user(PREF_AUDIO_MODE, mode.trim())
}

/// Playlist id when the active playback source is a user playlist, else None.
pub fn active_playlist_id(db: &Db) -> LibraryResult<Option<String>> {
    let src = source(db)?;
    Ok((src.source == QueueSourceKind::Playlist).then_some(src.playlist_id))
}

/// Record that `path` started playing (manual jumps included). Port of
/// plain-app `AudioQueueManager.onPlaying` — a manual jump breaks the cached
/// library position; it is marked unknown and re-located lazily on the next
/// sequential skip.
pub fn on_playing(
    db: &Db,
    path: &str,
    title: &str,
    artist: &str,
    duration_ms: i64,
) -> LibraryResult<()> {
    if path.is_empty() {
        return Ok(());
    }
    validate_duration(duration_ms)?;
    transaction(db, |c| {
        let mut src = io::get_source(c)?;
        if src.source == QueueSourceKind::Library
            && src.current_path != path
            && !io::all_queue_items(c)?.iter().any(|q| q.path == path)
        {
            src.current_index = -1;
        }
        src.current_path = path.into();
        io::save_source(c, &src)?;
        record_history_conn(c, path, title, artist, duration_ms)?;
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Manual queue
// ---------------------------------------------------------------------------

/// Add tracks to the manual queue. `play_next` moves/inserts them at the
/// front. Existing entries for the same path are moved, never duplicated.
pub fn enqueue(db: &Db, items: &[AudioTrack], play_next: bool) -> LibraryResult<()> {
    if items.is_empty() {
        return Ok(());
    }
    for item in items {
        validate_duration(item.duration_ms)?;
        if item.path.is_empty() {
            return Err(LibraryError::Other("empty audio path".into()));
        }
    }
    transaction(db, |c| {
        let mut queued = io::all_queue_items(c)?;
        let mut incoming = Vec::new();
        let mut seen = HashSet::new();
        for item in items.iter().rev() {
            if !seen.insert(&item.path) {
                continue;
            }
            incoming.push(QueueItem {
                path: item.path.clone(),
                sort_order: 0,
                title: item.title.clone(),
                artist: item.artist.clone(),
                duration_ms: item.duration_ms,
            });
        }
        incoming.reverse();
        queued.retain(|q| !seen.contains(&q.path));
        if play_next {
            incoming.extend(queued);
            queued = incoming;
        } else {
            queued.extend(incoming);
        }
        io::replace_queue_items(c, &queued)?;
        Ok(())
    })
}

pub fn remove_queued(db: &Db, path: &str) -> LibraryResult<()> {
    crate::db::audio_queue::remove_queue_item(db, path)?;
    Ok(())
}

/// Reorder the manual queue to match `paths`; unknown paths keep their
/// order at the end.
pub fn reorder_queued(db: &Db, paths: &[String]) -> LibraryResult<()> {
    if paths.is_empty() {
        return Ok(());
    }
    transaction(db, |c| {
        let all = io::all_queue_items(c)?;
        let mut seen = HashSet::new();
        let mut ordered = Vec::with_capacity(all.len());
        for path in paths {
            if seen.insert(path) {
                if let Some(item) = all.iter().find(|i| i.path == *path) {
                    ordered.push(item.clone());
                }
            }
        }
        ordered.extend(all.into_iter().filter(|i| !seen.contains(&i.path)));
        io::replace_queue_items(c, &ordered)?;
        Ok(())
    })
}

/// Cascade cleanup when media files are deleted or trashed.
pub fn remove_paths(db: &Db, paths: &[String]) -> LibraryResult<()> {
    if paths.is_empty() {
        return Ok(());
    }
    transaction(db, |c| {
        io::remove_queue_paths(c, paths)?;
        io::remove_history(c, paths)?;
        io::remove_playlist_items_by_paths(c, paths)?;
        let mut src = io::get_source(c)?;
        if paths.contains(&src.current_path) {
            src.current_path.clear();
            src.current_index = -1;
            io::save_source(c, &src)?;
        }
        Ok(())
    })
}

/// Reset the source and the manual queue. Stopping playback is the caller's
/// job (`clearAudioQueue` also clears the current track).
pub fn clear_queue(db: &Db) -> LibraryResult<()> {
    transaction(db, |c| {
        io::replace_queue_items(c, &[])?;
        io::save_source(c, &QueueSource::default())?;
        Ok(())
    })
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

fn source_size_of(db: &Db, lib: &mut dyn LibraryTracks, src: &QueueSource) -> LibraryResult<usize> {
    match src.source {
        QueueSourceKind::Playlist => Ok(crate::db::audio_queue::playlist_count(
            db,
            &src.playlist_id,
        )?),
        QueueSourceKind::Library => lib.library_count(),
        QueueSourceKind::None => Ok(0),
    }
}

fn current_pos_in_source(
    db: &Db,
    lib: &mut dyn LibraryTracks,
    src: &mut QueueSource,
    source_size: usize,
) -> LibraryResult<i64> {
    let path = src.current_path.clone();
    if path.is_empty() || source_size == 0 {
        return Ok(-1);
    }
    if crate::db::audio_queue::all_queue_items(db)?
        .iter()
        .any(|q| q.path == path)
    {
        return Ok(
            if src.current_index >= -1 && src.current_index < source_size as i64 {
                src.current_index
            } else {
                -1
            },
        );
    }
    match src.source {
        QueueSourceKind::Playlist => {
            Ok(
                crate::db::audio_queue::playlist_position(db, &src.playlist_id, &path)?
                    .unwrap_or(-1),
            )
        }
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
                save_source(db, src)?;
            }
            Ok(found)
        }
        QueueSourceKind::None => Ok(-1),
    }
}

fn playback_order(db: &Db, lib: &mut dyn LibraryTracks) -> LibraryResult<Order> {
    let mut src = source(db)?;
    let manual_count = crate::db::audio_queue::all_queue_items(db)?.len();
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
    db: &Db,
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
    db: &Db,
    lib: &mut dyn LibraryTracks,
    src: &QueueSource,
    rank: i64,
) -> LibraryResult<Option<AudioTrack>> {
    match src.source {
        QueueSourceKind::Playlist => {
            Ok(
                crate::db::audio_queue::playlist_items_page(db, &src.playlist_id, rank, 1)?
                    .first()
                    .map(AudioTrack::from),
            )
        }
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
    db: &Db,
    lib: &mut dyn LibraryTracks,
    order: &Order,
    queued: &[QueueItem],
) -> LibraryResult<HashSet<String>> {
    if order.manual_count == 0 || order.src.source == QueueSourceKind::None {
        return Ok(HashSet::new());
    }
    match order.src.source {
        QueueSourceKind::Playlist => {
            let mut out = HashSet::new();
            for q in queued {
                if crate::db::audio_queue::playlist_position(db, &order.src.playlist_id, &q.path)?
                    .is_some()
                {
                    out.insert(q.path.clone());
                }
            }
            Ok(out)
        }
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

fn next_source(db: &Db, order: &Order, rank: i64, path: &str) -> LibraryResult<QueueSource> {
    let manual_start = order.current_pos + 1;
    let source_pos: i64 = if rank >= manual_start && rank < manual_start + order.manual_count as i64
    {
        order.current_pos
    } else {
        match order.src.source {
            QueueSourceKind::Playlist => {
                crate::db::audio_queue::playlist_position(db, &order.src.playlist_id, path)?
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
        }
    };
    let mut src = order.src.clone();
    src.current_path = path.into();
    src.current_index = source_pos;
    Ok(src)
}

// ---------------------------------------------------------------------------
// Next / previous resolution
// ---------------------------------------------------------------------------

/// Resolve the next/previous track in the playback order and advance the
/// current track. Returns None when there is nothing to play.
pub fn resolve_next(
    db: &Db,
    lib: &mut dyn LibraryTracks,
    is_next: bool,
    shuffle: bool,
) -> LibraryResult<Option<AudioTrack>> {
    resolve_next_inner(db, lib, is_next, shuffle, true)
}
pub fn select_next(
    db: &Db,
    lib: &mut dyn LibraryTracks,
    is_next: bool,
    shuffle: bool,
) -> LibraryResult<Option<AudioTrack>> {
    resolve_next_inner(db, lib, is_next, shuffle, false)
}
fn resolve_next_inner(
    db: &Db,
    lib: &mut dyn LibraryTracks,
    is_next: bool,
    shuffle: bool,
    record_play: bool,
) -> LibraryResult<Option<AudioTrack>> {
    let order = playback_order(db, lib)?;
    if order.total() == 0 {
        return Ok(None);
    }
    let queued = crate::db::audio_queue::all_queue_items(db)?;
    let superseded = superseded_source_paths(db, lib, &order, &queued)?;
    let current = current_rank(&order, &queued);
    let total = order.total() as i64;
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
        let manual_start = order.current_pos + 1;
        let in_manual = target >= manual_start && target < manual_start + order.manual_count as i64;
        if in_manual || !superseded.contains(&a.path) {
            break;
        }
        target = if shuffle {
            use rand::Rng;
            rand::thread_rng().gen_range(0..total)
        } else if is_next {
            (target + 1) % total
        } else {
            (target - 1 + total) % total
        };
        let next = track_at(db, lib, &order, &queued, target)?;
        audio = next;
    }
    let audio = match audio {
        Some(a) => a,
        _ => return Ok(None),
    };
    let next = next_source(db, &order, target, &audio.path)?;
    transaction(db, |c| {
        if io::get_source(c)? != order.src {
            return Err(LibraryError::Other("playback source changed".into()));
        }
        io::save_source(c, &next)?;
        if record_play {
            record_history_conn(
                c,
                &audio.path,
                &audio.title,
                &audio.artist,
                audio.duration_ms,
            )?;
        }
        Ok(())
    })?;
    Ok(Some(audio))
}

// ---------------------------------------------------------------------------
// Queue totals / paging
// ---------------------------------------------------------------------------

pub fn queue_total(db: &Db, lib: &mut dyn LibraryTracks) -> LibraryResult<usize> {
    let order = playback_order(db, lib)?;
    let queued = crate::db::audio_queue::all_queue_items(db)?;
    let superseded = superseded_source_paths(db, lib, &order, &queued)?;
    Ok(order.total() - superseded.len())
}

/// A page of the playback order — never materializes the whole queue. When
/// `text` is set (the DSL `text:` field, case-insensitive substring over
/// title/artist/path) the order is paged through in chunks and only the
/// matching tracks are kept, so filtering precedes pagination.
pub fn queue_page(
    db: &Db,
    lib: &mut dyn LibraryTracks,
    offset: i64,
    limit: i64,
    text: &str,
) -> LibraryResult<Vec<AudioTrack>> {
    if offset < 0 || limit < 0 {
        return Err(LibraryError::Other("invalid pagination".into()));
    }
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
    db: &Db,
    lib: &mut dyn LibraryTracks,
    offset: i64,
    limit: i64,
) -> LibraryResult<Vec<AudioTrack>> {
    if offset < 0 || limit < 0 {
        return Err(LibraryError::Other("invalid pagination".into()));
    }
    if limit == 0 {
        return Ok(Vec::new());
    }
    let order = playback_order(db, lib)?;
    let queued = crate::db::audio_queue::all_queue_items(db)?;
    let superseded = superseded_source_paths(db, lib, &order, &queued)?;
    if superseded.is_empty() {
        return queue_page_raw(db, lib, offset, limit);
    }
    let mut out = Vec::new();
    let mut skipped = 0;
    let mut raw_offset = 0;
    while raw_offset < order.total() as i64 && out.len() < limit as usize {
        let page = queue_page_raw(db, lib, raw_offset, 500)?;
        let consumed = page.len();
        if consumed == 0 {
            break;
        }
        for item in page {
            let raw_rank = raw_offset;
            raw_offset += 1;
            let manual_start = order.current_pos + 1;
            let is_manual =
                raw_rank >= manual_start && raw_rank < manual_start + order.manual_count as i64;
            if !is_manual && superseded.contains(&item.path) {
                continue;
            }
            if skipped < offset {
                skipped += 1;
                continue;
            }
            out.push(item);
            if out.len() == limit as usize {
                break;
            }
        }
    }
    Ok(out)
}

fn queue_page_raw(
    db: &Db,
    lib: &mut dyn LibraryTracks,
    offset: i64,
    limit: i64,
) -> LibraryResult<Vec<AudioTrack>> {
    let order = playback_order(db, lib)?;
    let queued = crate::db::audio_queue::all_queue_items(db)?;
    let mut out: Vec<AudioTrack> = Vec::new();
    let mut rank = offset.max(0);
    let end = (offset + limit).min(order.total() as i64);
    while rank < end {
        if rank <= order.current_pos {
            // head: source up to the current track
            let seg_end = end.min(order.current_pos + 1);
            let rows = source_page(db, lib, &order.src, rank, seg_end - rank)?;
            out.extend(rows);
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
            out.extend(rows);
            rank = end;
        }
    }
    Ok(out)
}

fn source_page(
    db: &Db,
    lib: &mut dyn LibraryTracks,
    src: &QueueSource,
    offset: i64,
    limit: i64,
) -> LibraryResult<Vec<AudioTrack>> {
    if limit <= 0 {
        return Ok(vec![]);
    }
    match src.source {
        QueueSourceKind::Playlist => {
            Ok(
                crate::db::audio_queue::playlist_items_page(db, &src.playlist_id, offset, limit)?
                    .iter()
                    .map(AudioTrack::from)
                    .collect(),
            )
        }
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
    db: &Db,
    playlist_id: &str,
    start_path: Option<&str>,
) -> LibraryResult<Option<AudioTrack>> {
    set_playlist_source_inner(db, playlist_id, start_path, true)
}
pub fn select_playlist_source(
    db: &Db,
    playlist_id: &str,
    start_path: Option<&str>,
) -> LibraryResult<Option<AudioTrack>> {
    set_playlist_source_inner(db, playlist_id, start_path, false)
}
fn set_playlist_source_inner(
    db: &Db,
    playlist_id: &str,
    start_path: Option<&str>,
    record_play: bool,
) -> LibraryResult<Option<AudioTrack>> {
    transaction(db, |c| {
        if io::playlist_by_id(c, playlist_id)?.is_none() {
            return Err(LibraryError::Other("playlist not found".into()));
        }
        let items = io::playlist_items(c, playlist_id)?;
        io::replace_queue_items(c, &[])?;
        let Some(start) = start_path
            .and_then(|p| items.iter().find(|i| i.audio_path == p))
            .or_else(|| items.first())
        else {
            io::save_source(c, &QueueSource::default())?;
            return Ok(None);
        };
        let src = QueueSource {
            source: QueueSourceKind::Playlist,
            playlist_id: playlist_id.into(),
            current_path: start.audio_path.clone(),
            current_index: start.sort_order,
            ..QueueSource::default()
        };
        io::save_source(c, &src)?;
        let track = AudioTrack::from(start);
        if record_play {
            record_history_conn(
                c,
                &track.path,
                &track.title,
                &track.artist,
                track.duration_ms,
            )?;
        }
        Ok(Some(track))
    })
}

/// Play the whole library: make it the source, clear the manual queue.
/// Returns the track to start with.
pub fn set_library_source(
    db: &Db,
    lib: &mut dyn LibraryTracks,
    start_path: Option<&str>,
    shuffle: bool,
    sort: &str,
) -> LibraryResult<Option<AudioTrack>> {
    set_library_source_inner(db, lib, start_path, shuffle, sort, true)
}
pub fn select_library_source(
    db: &Db,
    lib: &mut dyn LibraryTracks,
    start_path: Option<&str>,
    shuffle: bool,
    sort: &str,
) -> LibraryResult<Option<AudioTrack>> {
    set_library_source_inner(db, lib, start_path, shuffle, sort, false)
}
fn set_library_source_inner(
    db: &Db,
    lib: &mut dyn LibraryTracks,
    start_path: Option<&str>,
    shuffle: bool,
    sort: &str,
    record_play: bool,
) -> LibraryResult<Option<AudioTrack>> {
    let size = lib.library_count()?;
    if size == 0 {
        clear_queue(db)?;
        return Ok(None);
    }
    // plain-app sorts the library source by the AudioSortByPreference
    // default (DATE_DESC).
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
            clear_queue(db)?;
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
    validate_duration(start.duration_ms)?;
    transaction(db, |c| {
        io::replace_queue_items(c, &[])?;
        io::save_source(c, &src)?;
        if record_play {
            record_history_conn(
                c,
                &start.path,
                &start.title,
                &start.artist,
                start.duration_ms,
            )?;
        }
        Ok(())
    })?;
    Ok(Some(start))
}

// ---------------------------------------------------------------------------
// User playlists
// ---------------------------------------------------------------------------

pub fn playlists(db: &Db) -> LibraryResult<Vec<(Playlist, usize)>> {
    let all = crate::db::audio_queue::all_playlists(db)?;
    let counts = crate::db::audio_queue::playlist_item_counts(db)?;
    Ok(all
        .into_iter()
        .map(|pl| {
            let count = counts.get(&pl.id).copied().unwrap_or(0);
            (pl, count)
        })
        .collect())
}

pub fn playlist_by_id(db: &Db, id: &str) -> LibraryResult<Option<Playlist>> {
    Ok(crate::db::audio_queue::playlist_by_id(db, id)?)
}

pub fn create_playlist(db: &Db, name: &str) -> LibraryResult<Playlist> {
    let now = now_iso_millis();
    let pl = Playlist {
        id: shortid::new_id(),
        name: name.to_string(),
        created_at: now.clone(),
        updated_at: now,
    };
    crate::db::audio_queue::insert_playlist(db, &pl)?;
    Ok(pl)
}

pub fn rename_playlist(db: &Db, id: &str, name: &str) -> LibraryResult<()> {
    transaction(db, |c| {
        let mut pl = io::playlist_by_id(c, id)?
            .ok_or_else(|| LibraryError::Other("playlist not found".into()))?;
        pl.name = name.into();
        pl.updated_at = now_iso_millis();
        io::update_playlist(c, &pl)?;
        Ok(())
    })
}

pub fn delete_playlist(db: &Db, id: &str) -> LibraryResult<()> {
    transaction(db, |c| {
        io::delete_playlist(c, id)?;
        io::delete_playlist_items(c, id)?;
        let mut src = io::get_source(c)?;
        if src.source == QueueSourceKind::Playlist && src.playlist_id == id {
            src.source = QueueSourceKind::None;
            src.playlist_id.clear();
            io::save_source(c, &src)?;
        }
        Ok(())
    })
}

/// Add tracks to a playlist; duplicates (same path) are ignored.
/// Returns how many were added.
pub fn add_playlist_items(
    db: &Db,
    playlist_id: &str,
    items: &[AudioTrack],
) -> LibraryResult<usize> {
    for item in items {
        validate_duration(item.duration_ms)?;
        if item.path.is_empty() {
            return Err(LibraryError::Other("empty audio path".into()));
        }
    }
    transaction(db, |c| {
        let mut pl = io::playlist_by_id(c, playlist_id)?
            .ok_or_else(|| LibraryError::Other("playlist not found".into()))?;
        let existing = io::playlist_items(c, playlist_id)?;
        let mut next = existing.last().map(|i| i.sort_order + 1).unwrap_or(0);
        let mut paths: HashSet<String> = existing.into_iter().map(|i| i.audio_path).collect();
        let mut added = 0;
        let now = now_iso_millis();
        for a in items {
            if !paths.insert(a.path.clone()) {
                continue;
            }
            let row = PlaylistItem {
                id: shortid::new_id(),
                playlist_id: playlist_id.into(),
                audio_path: a.path.clone(),
                title: a.title.clone(),
                artist: a.artist.clone(),
                duration_ms: a.duration_ms,
                album_id: a.album_id.clone(),
                sort_order: next,
                added_at: now.clone(),
            };
            io::insert_playlist_item(c, &row)?;
            next += 1;
            added += 1;
        }
        pl.updated_at = now;
        io::update_playlist(c, &pl)?;
        Ok(added)
    })
}

pub fn remove_playlist_item(db: &Db, playlist_id: &str, path: &str) -> LibraryResult<()> {
    transaction(db, |c| {
        io::remove_playlist_item(c, playlist_id, path)?;
        if let Some(mut pl) = io::playlist_by_id(c, playlist_id)? {
            pl.updated_at = now_iso_millis();
            io::update_playlist(c, &pl)?;
        }
        Ok(())
    })
}

pub fn playlist_items_page(
    db: &Db,
    playlist_id: &str,
    offset: i64,
    limit: i64,
    text: &str,
) -> LibraryResult<Vec<AudioTrack>> {
    if offset < 0 || limit < 0 {
        return Err(LibraryError::Other("invalid pagination".into()));
    }
    if text.trim().is_empty() {
        return Ok(
            crate::db::audio_queue::playlist_items_page(db, playlist_id, offset, limit)?
                .iter()
                .map(AudioTrack::from)
                .collect(),
        );
    }
    let needle = text.trim().to_lowercase();
    Ok(crate::db::audio_queue::playlist_items(db, playlist_id)?
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
        .collect())
}

pub fn playlist_item_count(db: &Db, playlist_id: &str) -> LibraryResult<usize> {
    Ok(crate::db::audio_queue::playlist_count(db, playlist_id)?)
}

// ---------------------------------------------------------------------------
// Play history
// ---------------------------------------------------------------------------

/// Recently played tracks, newest first, `text` filtering before paging.
pub fn history_page(
    db: &Db,
    offset: i64,
    limit: i64,
    text: &str,
) -> LibraryResult<Vec<PlayHistory>> {
    if offset < 0 || limit < 0 {
        return Err(LibraryError::Other("invalid pagination".into()));
    }
    let needle = text.trim().to_lowercase();
    Ok(crate::db::audio_queue::all_history(db)?
        .into_iter()
        .filter(|h| {
            needle.is_empty()
                || h.title.to_lowercase().contains(&needle)
                || h.artist.to_lowercase().contains(&needle)
                || h.path.to_lowercase().contains(&needle)
        })
        .skip(offset.max(0) as usize)
        .take(limit.max(0) as usize)
        .collect())
}

fn transaction<T>(
    db: &Db,
    operation: impl FnOnce(&rusqlite::Connection) -> LibraryResult<T>,
) -> LibraryResult<T> {
    db.with_conn(|c| {
        let tx = c.unchecked_transaction()?;
        let result = operation(&tx)?;
        tx.commit()?;
        Ok(result)
    })
}
fn validate_duration(value: i64) -> LibraryResult<()> {
    if value < 0 {
        return Err(LibraryError::Other("negative duration".into()));
    }
    Ok(())
}
fn record_history_conn(
    c: &rusqlite::Connection,
    path: &str,
    title: &str,
    artist: &str,
    duration_ms: i64,
) -> LibraryResult<()> {
    let count = io::history_by_path(c, path)?
        .map(|h| h.play_count)
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| LibraryError::Other("play count overflow".into()))?;
    io::upsert_history(
        c,
        &PlayHistory {
            path: path.into(),
            title: title.into(),
            artist: artist.into(),
            duration_ms,
            play_count: count,
            played_at: now_iso_millis(),
        },
    )?;
    let count: i64 = c.query_row("SELECT COUNT(*) FROM audio_play_history", [], |r| r.get(0))?;
    if count > (HISTORY_KEEP * 5 / 4) as i64 {
        io::trim_history(c, HISTORY_KEEP)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/library/audio_queue.rs"]
mod tests;

pub fn playlist_item_records_page(
    db: &Db,
    playlist_id: &str,
    offset: i64,
    limit: i64,
    text: &str,
) -> LibraryResult<Vec<PlaylistItem>> {
    if offset < 0 || limit < 0 {
        return Err(LibraryError::Other("invalid pagination".into()));
    }
    let needle = text.trim().to_lowercase();
    if needle.is_empty() {
        return Ok(crate::db::audio_queue::playlist_items_page(
            db,
            playlist_id,
            offset,
            limit,
        )?);
    }
    Ok(crate::db::audio_queue::playlist_items(db, playlist_id)?
        .into_iter()
        .filter(|i| {
            i.title.to_lowercase().contains(&needle)
                || i.artist.to_lowercase().contains(&needle)
                || i.audio_path.to_lowercase().contains(&needle)
        })
        .skip(offset as usize)
        .take(limit as usize)
        .collect())
}
pub fn remove_playlist_items(db: &Db, playlist_id: &str, paths: &[String]) -> LibraryResult<()> {
    transaction(db, |c| {
        for path in paths {
            io::remove_playlist_item(c, playlist_id, path)?;
        }
        if let Some(mut pl) = io::playlist_by_id(c, playlist_id)? {
            pl.updated_at = now_iso_millis();
            io::update_playlist(c, &pl)?;
        }
        Ok(())
    })
}
pub fn move_queued(db: &Db, from: i32, to: i32) -> LibraryResult<()> {
    transaction(db, |c| {
        let mut items = io::all_queue_items(c)?;
        if from < 0 || to < 0 || from as usize >= items.len() || to as usize >= items.len() {
            return Err(LibraryError::Other("invalid queue index".into()));
        }
        let item = items.remove(from as usize);
        items.insert(to as usize, item);
        for (i, item) in items.iter_mut().enumerate() {
            item.sort_order = i as i64;
        }
        io::replace_queue_items(c, &items)?;
        Ok(())
    })
}
pub fn record_history(
    db: &Db,
    path: &str,
    title: &str,
    artist: &str,
    duration_ms: i64,
) -> LibraryResult<()> {
    if path.is_empty() {
        return Err(LibraryError::Other("empty audio path".into()));
    }
    validate_duration(duration_ms)?;
    transaction(db, |c| {
        record_history_conn(c, path, title, artist, duration_ms)
    })
}
pub fn artist_play_counts(db: &Db) -> LibraryResult<std::collections::BTreeMap<String, i64>> {
    let mut out = std::collections::BTreeMap::<String, i64>::new();
    for row in crate::db::audio_queue::all_history(db)? {
        let total = out.entry(row.artist).or_default();
        *total = total
            .checked_add(row.play_count)
            .ok_or_else(|| LibraryError::Other("play count overflow".into()))?;
    }
    Ok(out)
}
