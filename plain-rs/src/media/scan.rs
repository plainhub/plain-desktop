//! Media scanner. 1:1 port of `internal/media/control.go` +
//! `internal/media/api.go` + the relevant pieces of `media_scan_api.go`.
//!
//! Differences from the Go side:
//! - **No FSUUID**: the Go side uses filesystem UUID + inode + ctime to derive
//!   a stable UUID that survives path renames. On Linux `nix` doesn't expose
//!   `st_ctim` as a `timespec` (we can read it via `statx` but the simpler
//!   solution is to derive UUID from a path hash). Since the MVP doesn't
//!   implement path-rename tracking, a path-hash UUID is fine.
//! - **No thumbnails**: ffmpeg integration is left as a 501 stub.
//! - **No metadata extraction at scan time**: duration/artist/title stay
//!   0/empty until a read path that surfaces them hydrates the row via
//!   `probe_missing_metadata` / `hydrate_metadata` (probe once, persist) —
//!   the same lazy read-side extraction the Go implementation does in its
//!   list resolvers.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicI32, AtomicI64, AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::{Mutex, Notify};


const KEY_PREFIX: &str = "media:uuid:";
const PATH_INDEX_PREFIX: &str = "media:path:";
/// Legacy write-only index (`media:type:{kind}:{uuid}`) that no reader ever
/// consumed. Still wiped by `reset_all` to clean pre-existing deployments.
const TYPE_INDEX_PREFIX: &str = "media:type:";

// ---------------------------------------------------------------------------
// Media-library exclusions
// ---------------------------------------------------------------------------

/// Path prefixes the media scan never descends into: system and program
/// areas whose icons/assets would swamp the media views (and most of which
/// are pure noise on a full-disk scan). The files manager lists the live
/// filesystem directly, so these stay browsable there — they are just not
/// part of the indexed media library.
const EXCLUDED_SYSTEM_ROOTS: &[&str] = &[
    "/proc", "/sys", "/dev", "/run", "/tmp", "/snap", "/usr", "/etc", "/var", "/boot", "/opt",
    "/srv", "/lib", "/lib32", "/lib64", "/bin", "/sbin",
];

/// Directory-name components excluded anywhere in the tree: build outputs
/// and vendored dependency trees hold program assets, never user media.
const EXCLUDED_DIR_NAMES: &[&str] = &["node_modules", "target", "dist", "build", "vendor"];

/// Extra excluded roots from config (`[media_scan] excluded_dirs`,
/// comma-separated absolute paths). Set once at startup.
static EXTRA_EXCLUDED_ROOTS: OnceLock<Vec<String>> = OnceLock::new();

/// Configure extra excluded roots (startup-only; applies to later scans).
pub fn set_extra_excluded_roots(roots: Vec<String>) {
    let _ = EXTRA_EXCLUDED_ROOTS.set(roots);
}

/// `true` if `path` must not enter the media library index. Hidden entries
/// (`.git`, `.cache`, the app's own `.nas-trash`, …) are excluded everywhere,
/// matching the phone MediaScanner semantics the watcher already follows.
pub fn is_media_excluded(path: &str) -> bool {
    let paths = crate::media::paths::detect();
    is_media_excluded_at(
        path,
        &paths.data_dir,
        &paths.cache_dir,
        EXTRA_EXCLUDED_ROOTS.get().map(Vec::as_slice).unwrap_or(&[]),
    )
}

fn is_media_excluded_at(
    path: &str,
    data_dir: &Path,
    cache_dir: &Path,
    extra_roots: &[String],
) -> bool {
    let p = path.replace('\\', "/");
    let is_hidden_or_named = |comp: &str| {
        comp.starts_with('.')
            || EXCLUDED_DIR_NAMES
                .iter()
                .any(|n| comp.eq_ignore_ascii_case(n))
    };
    if Path::new(&p)
        .components()
        .any(|c| c.as_os_str().to_str().is_some_and(is_hidden_or_named))
    {
        return true;
    }
    for root in EXCLUDED_SYSTEM_ROOTS {
        if under_root(&p, root) {
            return true;
        }
    }
    for root in extra_roots {
        if under_root(&p, root.trim_end_matches('/')) {
            return true;
        }
    }
    for dir in [data_dir, cache_dir] {
        let s = dir.to_string_lossy().replace('\\', "/");
        if under_root(&p, s.trim_end_matches('/')) {
            return true;
        }
    }
    false
}

/// Component-boundary prefix check: `/usr` covers `/usr/share/x` but not
/// `/usr2`; the root itself counts too.
fn under_root(p: &str, root: &str) -> bool {
    p == root || p.starts_with(&format!("{root}/"))
}

const STATE_IDLE: i32 = 0;
const STATE_RUNNING: i32 = 1;
const STATE_PAUSED: i32 = 2;
const STATE_STOPPED: i32 = 3;

/// Media-index scan lifecycle. The GraphQL `scanProgress.state` enum and
/// the `media:scan:progress` WS payload share the same `as_str()` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanState {
    Idle,
    Running,
    Paused,
    Stopped,
}

impl ScanState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ScanState::Idle => "IDLE",
            ScanState::Running => "RUNNING",
            ScanState::Paused => "PAUSED",
            ScanState::Stopped => "STOPPED",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MediaFile {
    pub uuid: String,
    /// Filesystem UUID (from /dev/disk/by-uuid via /proc/mounts).
    pub fsuuid: String,
    /// Inode number.
    pub ino: u64,
    /// ctime (seconds since epoch).
    pub ctime: i64,
    /// Cached duration in seconds (audio/video).
    pub duration_sec: u32,
    /// mtime when duration was cached.
    pub duration_ref_mod: i64,
    /// file size when duration was cached.
    pub duration_ref_size: i64,
    /// Artist tag (audio).
    pub artist: String,
    pub artist_ref_mod: i64,
    pub artist_ref_size: i64,
    /// Title tag (audio).
    pub title: String,
    pub title_ref_mod: i64,
    pub title_ref_size: i64,
    pub path: String,
    /// Original path before trash.
    pub original_path: String,
    pub name: String,
    pub size: i64,
    pub modified_at: i64,
    pub r#type: String, // "audio" | "video" | "image" | "doc" | "other"
    pub is_trash: bool,
    pub trash_path: String,
    pub deleted_at: i64,
}

pub struct Scanner {
    state: AtomicI32,
    /// Mirrors Go `pauseFlag`: when 1, the walk loop busy-waits
    /// (200ms sleep) until cleared by `resume()`. Independent from
    /// `stop_flag` so that pause does not accidentally clear a stop
    /// request, and vice versa.
    pause_flag: AtomicI32,
    stop_flag: AtomicI32,
    last_indexed: AtomicI64,
    last_total: AtomicI64,
    /// fjall commits issued by the current/last `scan_tree` run. The engine
    /// commits one batch per ~512 files; a count approaching the file count
    /// means someone reintroduced per-file commits (the pre-2026-09
    /// regression that made rebuilds minutes-slow). Read by tests.
    commits: AtomicU64,
    /// Absolute path (slash-normalised) of the directory the running
    /// scan is rooted at. Empty when no scan is in progress. Used to
    /// populate the `root` field of the `media:scan:progress` event
    /// payload (the Go side does the same via the ticker goroutine in
    /// `internal/media/scan.go`).
    current_root: Mutex<String>,
    /// Fires the per-second ticker task so the scan loop can shut it
    /// down deterministically when the walk finishes.
    ticker_done: Notify,
    running: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl Scanner {
    pub fn new() -> Self {
        Self {
            state: AtomicI32::new(STATE_IDLE),
            pause_flag: AtomicI32::new(0),
            stop_flag: AtomicI32::new(0),
            last_indexed: AtomicI64::new(0),
            last_total: AtomicI64::new(0),
            commits: AtomicU64::new(0),
            current_root: Mutex::new(String::new()),
            ticker_done: Notify::new(),
            running: Mutex::new(None),
        }
    }

    pub fn state(&self) -> ScanState {
        match self.state.load(Ordering::SeqCst) {
            STATE_RUNNING => {
                if self.stop_flag.load(Ordering::SeqCst) == 1 {
                    ScanState::Stopped
                } else if self.pause_flag.load(Ordering::SeqCst) == 1 {
                    // Defensive: matches Go's getStateString() which
                    // checks pauseFlag even when scanState==running
                    // (covers the tiny window between the two atomic
                    // stores in pause()).
                    ScanState::Paused
                } else {
                    ScanState::Running
                }
            }
            STATE_PAUSED => ScanState::Paused,
            STATE_STOPPED => ScanState::Stopped,
            _ => ScanState::Idle,
        }
    }

    pub fn get_progress(&self) -> (i64, i64, ScanState) {
        (
            self.last_indexed.load(Ordering::SeqCst),
            self.last_total.load(Ordering::SeqCst),
            self.state(),
        )
    }

    pub fn stop(&self) {
        self.stop_flag.store(1, Ordering::SeqCst);
        self.state.store(STATE_STOPPED, Ordering::SeqCst);
    }

    /// Mirrors Go `PauseScan`: set pauseFlag=1 and scanState=paused.
    /// Does NOT touch stopFlag (Go doesn't either) — pausing a stopped
    /// scan should not un-stop it.
    pub fn pause(&self) {
        self.pause_flag.store(1, Ordering::SeqCst);
        self.state.store(STATE_PAUSED, Ordering::SeqCst);
    }

    /// Mirrors Go `ResumeScan`: clear pauseFlag, set scanState=running.
    /// Does NOT touch stopFlag — if the scan was stopped, the walk has
    /// already exited and resume is a no-op from the walk's perspective.
    pub fn resume(&self) {
        self.pause_flag.store(0, Ordering::SeqCst);
        self.state.store(STATE_RUNNING, Ordering::SeqCst);
    }

    pub fn is_stopping(&self) -> bool {
        self.stop_flag.load(Ordering::SeqCst) == 1
    }

    pub fn is_paused(&self) -> bool {
        self.pause_flag.load(Ordering::SeqCst) == 1
    }

    pub async fn abort_running_task(&self) {
        if let Some(h) = self.running.lock().await.take() {
            h.abort();
            let _ = h.await;
        }
    }
}

impl Default for Scanner {
    fn default() -> Self {
        Self::new()
    }
}

// Process-wide singleton.
static SCANNER: std::sync::LazyLock<Arc<Scanner>> =
    std::sync::LazyLock::new(|| Arc::new(Scanner::new()));

pub fn scanner() -> Arc<Scanner> {
    SCANNER.clone()
}

// ---------------------------------------------------------------------------
// Type inference
// ---------------------------------------------------------------------------

pub fn infer_type(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    let ext = std::path::Path::new(&lower)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match ext {
        "mp3" | "wav" | "wma" | "ogg" | "m4a" | "opus" | "flac" | "aac" => "audio",
        "mp4" | "mkv" | "webm" | "avi" | "3gp" | "mov" | "m4v" | "3gpp" => "video",
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "heic" | "heif"
        | "avif" | "svg" => "image",
        // Docs are classified off the shared MIME table (plain-rs, same
        // source plain-app's doc queries use): anything `text/*` plus the
        // structured-text / office formats from plain-app
        // `extraDocumentMimeTypes` (pdf / doc / docx / xlsx / js) and the
        // application/* types the shared table emits for .json/.xml.
        // Derived, never a second hand-maintained ext list.
        _ => {
            let mime = crate::utils::mime::mime_from_ext(name);
            if mime.starts_with("text/") || DOC_EXTRA_MIMES.contains(&mime) {
                "doc"
            } else {
                "other"
            }
        }
    }
}

/// Non-`text/*` MIME types that still classify a file as a document:
/// plain-app `DocMediaStoreHelper.extraDocumentMimeTypes` plus
/// `application/json` / `application/xml`.
const DOC_EXTRA_MIMES: [&str; 7] = [
    "application/pdf",
    "application/msword",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "application/javascript",
    "application/json",
    "application/xml",
];

/// Lowercased extension of `name` ("" when extensionless). The index-side
/// identity of a document file: powers the `ext:` search filter and the
/// `docExtGroups` sidebar aggregation.
pub fn ext_of(name: &str) -> String {
    let lower = name.to_lowercase();
    match lower.rsplit_once('.') {
        Some((_, ext)) => ext.to_string(),
        None => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Upsert a single file
// ---------------------------------------------------------------------------

pub fn scan_file(db: &crate::media::kv::Db, path: &str) -> Result<MediaFile> {
    if is_media_excluded(path) {
        return Err(anyhow::anyhow!(
            "path is excluded from the media index: {path}"
        ));
    }
    let meta = std::fs::metadata(path)?;
    if !meta.is_file() {
        return Err(anyhow::anyhow!("not a regular file: {path}"));
    }
    let row = build_scanned(db, path, &meta)?;
    let mut batch = db.batch();
    stage_scanned(&mut batch, &row);
    db.apply_batch(batch)?;
    apply_bucket_deltas(db, &bucket_deltas_of(std::slice::from_ref(&row)));
    let _ = crate::media::image_index::global().index_media_file(&row.m);
    Ok(row.m)
}

/// A fully resolved index write, ready to be staged into a batch without any
/// further DB access. Workers produce these; a single writer stages them, so
/// all per-file reads happen off the write path.
struct ScannedFile {
    m: MediaFile,
    /// `m` pre-serialized (workers do the JSON encode in parallel).
    bytes: Vec<u8>,
    /// Stale `media:path:` row to drop when the uuid's path changed.
    old_path_key: Option<String>,
    /// Stale `media:fid:` row to drop when (fsuuid, ino, ctime) changed.
    old_fid_key: Option<String>,
    /// Previous bucket (type, dir) when the row moves out of one.
    bucket_old: Option<(String, String)>,
    /// Current bucket (type, dir).
    bucket_new: Option<(String, String)>,
}

/// Aggregate per-bucket counter deltas for a set of scanned rows. Applied by
/// the single committer thread (read-modify-write must be serial), so batch
/// staging stays race-free without per-file counter reads.
fn bucket_deltas_of(rows: &[ScannedFile]) -> HashMap<(String, String), i64> {
    let mut deltas: HashMap<(String, String), i64> = HashMap::new();
    for row in rows {
        if let Some(old) = &row.bucket_old {
            *deltas.entry(old.clone()).or_insert(0) -= 1;
        }
        if let Some(new) = &row.bucket_new {
            *deltas.entry(new.clone()).or_insert(0) += 1;
        }
    }
    deltas
}

/// Apply bucket counter deltas. Single-threaded only (the committer thread
/// or single-file scan paths) — the get/modify/set cycle is not atomic.
fn apply_bucket_deltas(db: &crate::media::kv::Db, deltas: &HashMap<(String, String), i64>) {
    for ((kind, dir), delta) in deltas {
        let key = crate::media::image_index::bucket_key(kind, dir);
        let cur = db
            .get(key.as_bytes())
            .ok()
            .flatten()
            .and_then(|v| String::from_utf8_lossy(v.as_ref()).parse::<i64>().ok())
            .unwrap_or(0);
        let next = cur + delta;
        if next > 0 {
            let _ = db.insert(key.as_bytes(), next.to_string().as_bytes());
        } else {
            let _ = db.remove(key.as_bytes());
        }
    }
}

/// Bucket grouping applies to normal library rows only — trashed rows live
/// under `.nas-trash` and must not show up as (or inflate) buckets.
fn bucket_of_row(m: &MediaFile) -> Option<(String, String)> {
    if m.is_trash || !crate::media::image_index::bucketed_type(&m.r#type) {
        None
    } else {
        Some((
            m.r#type.clone(),
            crate::media::image_index::parent_dir_of(&m.path),
        ))
    }
}

/// Persist a modified MediaFile row (trash / restore flows): rewrites the
/// primary row, fixes the path index when the path changed, and syncs the
/// derived stores (bucket counters + tantivy mirror). The KV batch commits
/// first; derived stores follow only on success.
pub fn upsert_media_row(db: &crate::media::kv::Db, m: &MediaFile) -> Result<()> {
    let old = get_by_uuid(db, &m.uuid)?;
    let old_path_key = old
        .as_ref()
        .filter(|o| o.path != m.path)
        .map(|o| format!("{PATH_INDEX_PREFIX}{}", o.path));
    let old_fid_key = old
        .as_ref()
        .filter(|o| {
            !o.fsuuid.is_empty() && (o.fsuuid != m.fsuuid || o.ino != m.ino || o.ctime != m.ctime)
        })
        .map(|o| crate::media::uuid::fid_key(&o.fsuuid, o.ino, o.ctime));
    let bucket_old = old.as_ref().and_then(|o| bucket_of_row(o));
    let bucket_new = bucket_of_row(m);
    let (bucket_old, bucket_new) = match (bucket_old, bucket_new) {
        (Some(o), Some(n)) if o == n => (None, None),
        pair => pair,
    };

    let row = ScannedFile {
        m: m.clone(),
        bytes: serde_json::to_vec(m)?,
        old_path_key,
        old_fid_key,
        bucket_old,
        bucket_new,
    };
    let mut batch = db.batch();
    stage_scanned(&mut batch, &row);
    db.apply_batch(batch)?;
    apply_bucket_deltas(db, &bucket_deltas_of(std::slice::from_ref(&row)));
    let _ = crate::media::image_index::global().index_media_file(&row.m);
    Ok(())
}

// ---------------------------------------------------------------------------
// Read-side metadata hydration (lazy probe + persist)
// ---------------------------------------------------------------------------

/// Production probe adapter: one probe pass per row (`probe_media` parses an
/// audio file once for duration+artist+title).
fn probe_row(mf: &MediaFile) -> crate::media::metadata::ProbedMeta {
    crate::media::metadata::probe_media(&mf.path, &mf.r#type)
}

/// Whether the row's metadata ref stamps still describe the current file
/// version — fresh stamps mean "probed at this (mtime, size)" and no probe
/// is due, regardless of what the probe found (nothing counts as an answer;
/// the Go `Ensure*` helpers re-parse tagless files on every read instead).
fn metadata_refs_stale(mf: &MediaFile) -> bool {
    let fresh = |r#mod, size| r#mod == mf.modified_at && size == mf.size;
    match mf.r#type.as_str() {
        "video" => !(fresh(mf.duration_ref_mod, mf.duration_ref_size)),
        "audio" => {
            !(fresh(mf.duration_ref_mod, mf.duration_ref_size)
                && fresh(mf.artist_ref_mod, mf.artist_ref_size)
                && fresh(mf.title_ref_mod, mf.title_ref_size))
        }
        _ => false,
    }
}

/// Refresh stale metadata on a media row in place via `probe` (injection
/// seam; production = one file parse per row).
///
/// Caching is keyed by (mtime, size) ref stamps and the stamp is written on
/// **every** attempt, including probes that find nothing, so each file
/// version is probed at most once. Returns true when a probe ran and the
/// row should be persisted.
pub fn probe_missing_metadata_with(
    mf: &mut MediaFile,
    probe: fn(&MediaFile) -> crate::media::metadata::ProbedMeta,
) -> bool {
    if !metadata_refs_stale(mf) {
        return false;
    }
    let m = probe(mf);
    if m.parsed {
        // The file was read: its values are authoritative, empty tags
        // included (no tag ≠ unknown).
        mf.duration_sec = m.duration_secs;
        if mf.r#type == "audio" {
            mf.artist = m.artist;
            mf.title = m.title;
        }
    } else {
        // Nothing could read the file (missing / unrecognized): keep the
        // cached values — an unmounted disk must not zero the library —
        // except a container-walk duration, which did read it.
        if m.duration_secs > 0 {
            mf.duration_sec = m.duration_secs;
        }
    }
    // Refs stamp either way: a file that cannot be read is not re-read on
    // every view; it re-probes only when (mtime, size) change.
    mf.duration_ref_mod = mf.modified_at;
    mf.duration_ref_size = mf.size;
    if mf.r#type == "audio" {
        mf.artist_ref_mod = mf.modified_at;
        mf.artist_ref_size = mf.size;
        mf.title_ref_mod = mf.modified_at;
        mf.title_ref_size = mf.size;
    }
    true
}

/// `probe_missing_metadata_with` with the production probe.
pub fn probe_missing_metadata(mf: &mut MediaFile) -> bool {
    probe_missing_metadata_with(mf, probe_row)
}

/// `probe_missing_metadata` + persist the row (plain-app/Go pattern: each
/// file is probed at most once per (mtime, size), from the read path that
/// actually surfaces it — the scan never blocks on metadata).
pub fn hydrate_metadata(db: &crate::media::kv::Db, mf: &mut MediaFile) -> bool {
    if !probe_missing_metadata(mf) {
        return false;
    }
    if let Err(e) = persist_metadata_rows(db, std::slice::from_ref(mf)) {
        log::warn!("[media] hydrate persist {} failed: {e}", mf.path);
        return false;
    }
    true
}

/// Fill missing metadata for a page of search hits in place, persisting all
/// probed rows in one batched write (single fjall batch + single index
/// commit). Hits that already carry everything — the steady state after the
/// first view — are skipped without touching the KV store at all.
pub fn hydrate_search_page(
    db: &crate::media::kv::Db,
    hits: &mut [crate::media::image_index::MediaSearchResult],
    audio: bool,
) -> usize {
    hydrate_search_page_with(db, hits, audio, probe_row)
}

/// `hydrate_search_page` with an injected probe fn (tests count probes).
pub fn hydrate_search_page_with(
    db: &crate::media::kv::Db,
    hits: &mut [crate::media::image_index::MediaSearchResult],
    audio: bool,
    probe: fn(&MediaFile) -> crate::media::metadata::ProbedMeta,
) -> usize {
    // Phase 1 — resolve incomplete hits to rows (cheap KV point reads).
    // Rows with fresh ref stamps never probe: they are served straight from
    // the row (the index doc can lag it) and never enter the thread pool —
    // the steady state pays KV reads only, zero threads.
    let mut jobs: Vec<(usize, MediaFile, bool)> = Vec::new();
    let mut served_fresh = 0;
    for (i, hit) in hits.iter_mut().enumerate() {
        let complete =
            hit.duration_secs > 0 && (!audio || (!hit.artist.is_empty() && !hit.title.is_empty()));
        if complete {
            continue;
        }
        let Ok(Some(mf)) = get_by_uuid(db, &hit.uuid) else {
            continue;
        };
        if metadata_refs_stale(&mf) {
            jobs.push((i, mf, false));
        } else {
            hit.duration_secs = mf.duration_sec;
            hit.artist = mf.artist.clone();
            hit.title = mf.title.clone();
            served_fresh += 1;
        }
    }
    let _ = served_fresh;
    if jobs.is_empty() {
        return 0;
    }
    // Phase 2 — probe. Probes are independent file reads with no shared
    // state, so a cold page (the only slow case: one lofty parse per file)
    // parallelizes across cores; a stale-doc page skips straight past
    // (fresh ref stamps) at KV-read cost.
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(jobs.len());
    if threads <= 1 {
        for (_, mf, probed) in jobs.iter_mut() {
            *probed = probe_missing_metadata_with(mf, probe);
        }
    } else {
        let chunk = jobs.len().div_ceil(threads);
        std::thread::scope(|s| {
            let handles: Vec<_> = jobs
                .chunks_mut(chunk)
                .map(|c| {
                    s.spawn(move || {
                        for (_, mf, probed) in c.iter_mut() {
                            *probed = probe_missing_metadata_with(mf, probe);
                        }
                    })
                })
                .collect();
            for h in handles {
                if let Err(p) = h.join() {
                    std::panic::resume_unwind(p);
                }
            }
        });
    }
    // Phase 3 — serve values from the rows and persist what probed, in one
    // fjall batch + one index commit.
    let mut probed: Vec<MediaFile> = Vec::new();
    for (i, mf, did) in jobs {
        // Serve the row's values even when this pass probed nothing new —
        // the index doc can lag the row until the persist below lands.
        let hit = &mut hits[i];
        hit.duration_secs = mf.duration_sec;
        hit.artist = mf.artist.clone();
        hit.title = mf.title.clone();
        if did {
            probed.push(mf);
        }
    }
    if let Err(e) = persist_metadata_rows(db, &probed) {
        log::warn!("[media] hydrate batch persist failed: {e}");
    }
    probed.len()
}

/// Persist metadata-only row updates in one fjall batch + one index commit.
/// Hydration never changes identity (path/uuid/fid) or bucket assignment,
/// so unlike a full re-upsert this writes exactly **one** KV row per file —
/// the `media:path:` / `media:fid:` secondaries are never rewritten.
fn persist_metadata_rows(db: &crate::media::kv::Db, rows: &[MediaFile]) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let index = crate::media::image_index::global();
    let mut batch = db.batch();
    for m in rows {
        batch.insert(
            format!("{KEY_PREFIX}{}", m.uuid).as_bytes(),
            serde_json::to_vec(m)?.as_slice(),
        );
        index.add_media_file(m)?;
    }
    db.apply_batch(batch)?;
    index.commit()
}

/// Resolve identity, preserve cached metadata from the previous entry and
/// serialize the row. Does two point reads (`media:fid:` + old `media:uuid:`)
/// and no writes — safe to call from many threads concurrently.
fn build_scanned(db: &crate::media::kv::Db, path: &str, meta: &std::fs::Metadata) -> Result<ScannedFile> {
    let name = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let kind = infer_type(&name);
    let size = meta.len() as i64;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // Stable UUID from (FSUUID, inode, ctime) — the stat is already done, so
    // no extra syscall here.
    let (derived, fsuuid, ino, ctime) = crate::media::uuid::generate_uuid_from_metadata(path, meta);

    // Reuse an existing UUID when the FID secondary index knows this file.
    let uuid = crate::media::uuid::find_uuid_by_fid(db, &fsuuid, ino, ctime).unwrap_or(derived);

    let old: Option<MediaFile> = db
        .get(format!("{KEY_PREFIX}{uuid}").as_bytes())?
        .and_then(|v| serde_json::from_slice::<MediaFile>(v.as_ref()).ok());

    // Preserve cached tag/duration metadata from the old entry, and note the
    // stale secondary-index rows it forces us to drop.
    let old_path_key = old
        .as_ref()
        .filter(|m| m.path != path)
        .map(|m| format!("{PATH_INDEX_PREFIX}{}", m.path));
    let old_fid_key = old
        .as_ref()
        .filter(|m| {
            !m.fsuuid.is_empty() && (m.fsuuid != fsuuid || m.ino != ino || m.ctime != ctime)
        })
        .map(|m| crate::media::uuid::fid_key(&m.fsuuid, m.ino, m.ctime));
    let (
        duration_sec,
        duration_ref_mod,
        duration_ref_size,
        artist,
        artist_ref_mod,
        artist_ref_size,
        title,
        title_ref_mod,
        title_ref_size,
    ) = old
        .as_ref()
        .map(|m| {
            (
                m.duration_sec,
                m.duration_ref_mod,
                m.duration_ref_size,
                m.artist.clone(),
                m.artist_ref_mod,
                m.artist_ref_size,
                m.title.clone(),
                m.title_ref_mod,
                m.title_ref_size,
            )
        })
        .unwrap_or_default();

    let m = MediaFile {
        uuid,
        fsuuid,
        ino,
        ctime,
        duration_sec,
        duration_ref_mod,
        duration_ref_size,
        artist,
        artist_ref_mod,
        artist_ref_size,
        title,
        title_ref_mod,
        title_ref_size,
        path: path.to_string(),
        original_path: path.to_string(),
        name,
        size,
        modified_at: mtime,
        r#type: kind.to_string(),
        is_trash: false,
        trash_path: String::new(),
        deleted_at: 0,
    };
    let bytes = serde_json::to_vec(&m)?;
    let bucket_new = crate::media::image_index::bucketed_type(kind).then(|| {
        (
            kind.to_string(),
            crate::media::image_index::parent_dir_of(path),
        )
    });
    let bucket_old = old.as_ref().and_then(|m| bucket_of_row(m));
    // A re-scan of an unchanged row must not move the counter.
    let (bucket_old, bucket_new) = match (bucket_old, bucket_new) {
        (Some(o), Some(n)) if o == n => (None, None),
        pair => pair,
    };
    Ok(ScannedFile {
        m,
        bytes,
        old_path_key,
        old_fid_key,
        bucket_old,
        bucket_new,
    })
}

/// Stage every index row for `row` into `batch`. Pure key/value writes — no
/// DB reads, no commits — so the scan writer can amortize one fjall commit
/// over thousands of files.
fn stage_scanned(batch: &mut crate::media::kv::Batch, row: &ScannedFile) {
    let m = &row.m;
    batch.insert(
        format!("{KEY_PREFIX}{}", m.uuid).as_bytes(),
        row.bytes.as_slice(),
    );
    if let Some(k) = &row.old_path_key {
        batch.remove(k.as_bytes());
    }
    batch.insert(
        format!("{PATH_INDEX_PREFIX}{}", m.path).as_bytes(),
        m.uuid.as_bytes(),
    );
    if !m.fsuuid.is_empty() {
        if let Some(k) = &row.old_fid_key {
            batch.remove(k.as_bytes());
        }
        batch.insert(
            crate::media::uuid::fid_key(&m.fsuuid, m.ino, m.ctime).as_bytes(),
            m.uuid.as_bytes(),
        );
    }
}

pub fn get_by_uuid(db: &crate::media::kv::Db, uuid: &str) -> Result<Option<MediaFile>> {
    let key = format!("{KEY_PREFIX}{uuid}");
    Ok(db.get(&key)?.and_then(|v| serde_json::from_slice(&v).ok()))
}

pub fn get_by_path(db: &crate::media::kv::Db, path: &str) -> Result<Option<MediaFile>> {
    let key = format!("{PATH_INDEX_PREFIX}{path}");
    let Some(uuid) = db.get(&key)? else {
        return Ok(None);
    };
    let uuid = String::from_utf8_lossy(&uuid).to_string();
    get_by_uuid(db, &uuid)
}

pub fn delete_by_uuid(db: &crate::media::kv::Db, uuid: &str) -> Result<()> {
    if let Some(m) = get_by_uuid(db, uuid)? {
        let mut batch = db.batch();
        batch.remove(format!("{KEY_PREFIX}{uuid}").as_bytes());
        batch.remove(format!("{PATH_INDEX_PREFIX}{}", m.path).as_bytes());
        // Legacy rows from before the type index was dropped.
        if !m.r#type.is_empty() && m.r#type != "other" {
            batch.remove(format!("{TYPE_INDEX_PREFIX}{}:{}", m.r#type, uuid).as_bytes());
        }
        // The FID row must go too, or every deleted file leaks one index
        // entry forever.
        if !m.fsuuid.is_empty() {
            batch.remove(crate::media::uuid::fid_key(&m.fsuuid, m.ino, m.ctime).as_bytes());
        }
        db.apply_batch(batch)?;
        // Derived stores: bucket counter + tantivy mirror. Trashed rows never
        // counted in buckets, so only non-trash deletions decrement.
        if let Some((kind, dir)) = bucket_of_row(&m) {
            let deltas = HashMap::from([((kind, dir), -1i64)]);
            apply_bucket_deltas(db, &deltas);
        }
        let _ = crate::media::image_index::global().remove_by_uuid(&m.uuid);
    }
    Ok(())
}

/// Delete a single media index entry by path. Mirrors Go's `media.RemovePath`:
/// look up the UUID via the path index, then call `delete_by_uuid`. Returns
/// `Ok(true)` if a row was deleted, `Ok(false)` if no entry matched.
pub fn delete_by_path(db: &crate::media::kv::Db, path: &str) -> Result<bool> {
    let normalized = path.replace('\\', "/");
    let key = format!("{PATH_INDEX_PREFIX}{normalized}");
    let Some(uuid_ivec) = db.get(&key)? else {
        return Ok(false);
    };
    let uuid = String::from_utf8_lossy(&uuid_ivec).to_string();
    delete_by_uuid(db, &uuid)?;
    Ok(true)
}

/// Purge every media index entry whose path starts with `prefix`. Mirrors
/// Go's `purgeMediaIndexByPathPrefix` (used when a directory is deleted or
/// trashed so all of its children stop showing up in the media library).
pub fn delete_by_path_prefix(db: &crate::media::kv::Db, prefix: &str) -> Result<usize> {
    let mut normalized = prefix.replace('\\', "/");
    if !normalized.is_empty() && !normalized.ends_with('/') {
        normalized.push('/');
    }
    if normalized.is_empty() {
        return Ok(0);
    }
    let scan_prefix = format!("{PATH_INDEX_PREFIX}{normalized}");
    // (path-index key, uuid) pairs; the value already IS the uuid, so the
    // purge needs one `get` per file, not two.
    let mut to_delete: Vec<(String, String)> = Vec::new();
    for kv in db.scan_prefix(&scan_prefix) {
        let (k, v) = kv?;
        let k = String::from_utf8_lossy(&k).to_string();
        let v = String::from_utf8_lossy(&v).to_string();
        to_delete.push((k, v));
    }
    let mut batch = db.batch();
    for (k, _v) in &to_delete {
        batch.remove(k.as_bytes());
    }
    // For each matched path, also drop the media:uuid:/fid: rows (plus
    // legacy type rows). Collect the rows first — the derived-store updates
    // below need their type/path/uuid.
    let mut purged_rows: Vec<MediaFile> = Vec::new();
    for (k, uuid) in &to_delete {
        let _ = k;
        if let Some(m) = get_by_uuid(db, uuid)? {
            batch.remove(format!("{KEY_PREFIX}{}", m.uuid).as_bytes());
            if !m.r#type.is_empty() && m.r#type != "other" {
                batch.remove(format!("{TYPE_INDEX_PREFIX}{}:{}", m.r#type, m.uuid).as_bytes());
            }
            if !m.fsuuid.is_empty() {
                batch.remove(crate::media::uuid::fid_key(&m.fsuuid, m.ino, m.ctime).as_bytes());
            }
            purged_rows.push(m);
        }
    }
    db.apply_batch(batch)?;
    // Derived stores: bucket counters + tantivy mirror.
    let mut deltas: HashMap<(String, String), i64> = HashMap::new();
    let mut uuids: Vec<&str> = Vec::with_capacity(purged_rows.len());
    for m in &purged_rows {
        uuids.push(&m.uuid);
        if let Some((kind, dir)) = bucket_of_row(m) {
            *deltas.entry((kind, dir)).or_insert(0) -= 1;
        }
    }
    apply_bucket_deltas(db, &deltas);
    let _ = crate::media::image_index::global().remove_by_uuids(&uuids);
    Ok(to_delete.len())
}

pub fn reset_all(db: &crate::media::kv::Db) -> Result<()> {
    // Wipe via atomic batches: one journal commit per chunk instead of one
    // per key — with ~2M index rows for a 600k-file library the per-key
    // path alone stalled the rebuild for the better part of a minute.
    const CHUNK: usize = 10_000;
    for prefix in [
        KEY_PREFIX,
        PATH_INDEX_PREFIX,
        TYPE_INDEX_PREFIX,
        crate::media::uuid::FID_INDEX_PREFIX,
        crate::media::image_index::BUCKET_PREFIX,
    ] {
        let mut batch = db.batch();
        let mut chunked = 0usize;
        for kv in db.scan_prefix(prefix) {
            let (k, _) = kv?;
            batch.remove(&k[..]);
            chunked += 1;
            if chunked.is_multiple_of(CHUNK) {
                db.apply_batch(std::mem::replace(&mut batch, db.batch()))?;
            }
        }
        db.apply_batch(batch)?;
    }
    // Derived store: the tantivy mirror is rebuilt from scratch by the scan
    // that follows the reset.
    let _ = crate::media::image_index::global().clear();
    Ok(())
}

// ---------------------------------------------------------------------------
// Async walker
// ---------------------------------------------------------------------------

/// Start a background walk+scan of `root`. The previous in-flight task is
/// aborted. Progress is published via `eventbus::publish_with_cid` on the
/// "media:scan:progress" channel, every 1 second by a dedicated ticker
/// task (mirrors the `time.NewTicker(time.Second)` goroutine in
/// `internal/media/scan.go`).
pub async fn start_walk_and_scan(db: Arc<crate::media::kv::Db>, root: std::path::PathBuf) -> Result<()> {
    log::info!(
        "[scan] start_walk_and_scan enter root={}",
        path_to_slash(&root)
    );
    let s = scanner();
    s.abort_running_task().await;
    // Reset both flags for a fresh scan, mirroring Go's ScanAndSync
    // which does `stopFlag=0; pauseFlag=0; setStateRunning()` at the
    // top. We can't just call `s.resume()` because resume() no longer
    // touches stop_flag (matching Go's ResumeScan which also doesn't).
    s.stop_flag.store(0, Ordering::SeqCst);
    s.pause_flag.store(0, Ordering::SeqCst);
    s.state.store(STATE_RUNNING, Ordering::SeqCst);
    s.last_indexed.store(0, Ordering::SeqCst);
    s.last_total.store(0, Ordering::SeqCst);
    // Persist the root for the duration of the scan so the progress
    // events (ticker + final) can include it. The Go side carries `root`
    // in a local variable into the ticker closure; we use a field on
    // `Scanner` because the stop / pause / state-change events also want
    // to know what root the *last* scan was running (mirrors Go's
    // `getStateString`-style global view).
    *s.current_root.lock().await = path_to_slash(&root);
    log::info!(
        "[scan] scanner state set: running root={}",
        path_to_slash(&root)
    );

    // Per-second progress ticker. Mirrors `ticker := time.NewTicker(1s)`
    // in the Go side. The scan loop signals completion via
    // `scanner().ticker_done.notify_waiters()`. **Spawned before the
    // precount so the UI gets a steady stream of events during the
    // (potentially long) precount**: this matches the Go side, where
    // the ticker goroutine is started inside `ScanAndSync` itself and
    // the WalkDir runs in the same goroutine that publishes the final
    // event — the ticker never blocks on the walk.
    {
        let s2 = s.clone();
        log::info!("[scan] spawning 1s ticker task");
        tokio::task::spawn(async move {
            log::info!("[scan] ticker task started");
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {
                        let st = s2.state();
                        log::info!("[scan] ticker tick state={} indexed={} total={} root={:?}",
                            st.as_str(),
                            s2.last_indexed.load(Ordering::SeqCst),
                            s2.last_total.load(Ordering::SeqCst),
                            s2.current_root.try_lock().ok().map(|g| g.clone()).unwrap_or_default());
                        if matches!(st, ScanState::Idle | ScanState::Stopped) { break; }
                        publish_progress(&s2);
                    }
                    _ = s2.ticker_done.notified() => {
                        log::info!("[scan] ticker notified, exit");
                        break;
                    }
                }
            }
            log::info!("[scan] ticker task ended");
        });
    }

    // Pre-count files for progress. Mirrors
    // `consts.ENABLE_SCAN_PRECOUNT` in the Go side. The walk itself uses
    // `std::fs` (synchronous IO) and would block the tokio worker thread
    // for the full duration of a large library if we did it directly.
    // `spawn_blocking` parks it on tokio's dedicated blocking thread
    // pool — same scheduling shape as the Go goroutine doing the
    // WalkDir concurrently with the ticker goroutine.
    let root_for_count = root.clone();
    log::info!("[scan] spawning precount on blocking pool");
    let total: i64 = tokio::task::spawn_blocking(move || {
        let t = count_files(&root_for_count);
        log::info!("[scan] precount done total={}", t);
        t
    })
    .await
    .map_err(|e| anyhow::anyhow!("precount join error: {e}"))?;
    s.last_total.store(total, Ordering::SeqCst);
    publish_progress(&s);

    let db2 = db.clone();
    let s2 = s.clone();
    let root2 = root.clone();
    log::info!("[scan] spawning walk task on blocking pool");
    let handle = tokio::task::spawn(async move {
        // The walk + per-file scan_file both do synchronous IO
        // (read_dir / stat / KV batch writes). Run them on the
        // blocking thread pool so they can't starve the main
        // tokio workers (where the GraphQL resolvers and WS writer
        // tasks live). This mirrors the Go side, where WalkDir runs
        // on its own goroutine and the goroutine scheduler can
        // preempt it on every syscall.
        let db3 = db2.clone();
        let s3 = s2.clone();
        let root3 = root2.clone();
        log::info!("[scan] walk task started root={}", path_to_slash(&root3));
        let walk_result = tokio::task::spawn_blocking(move || scan_tree(&db3, &root3, &s3)).await;
        match walk_result {
            Ok((files_seen, indexed)) => log::info!(
                "[scan] walk done files_seen={} indexed={}",
                files_seen,
                indexed
            ),
            Err(e) => log::error!("[scan] walk task join error: {e}"),
        }
        // Mark the state machine idle before the final progress push so
        // the event the UI sees has `state:"IDLE"`. Mirrors the Go side's
        // `setStateIdle()` call right before the final publish.
        s2.state.store(STATE_IDLE, Ordering::SeqCst);
        s2.ticker_done.notify_waiters();
        publish_progress(&s2);
        // A finished scan means fresh topItems: warm the bucket-grid
        // thumbnails in the background (no-op when prefetching is disabled).
        crate::media::thumb::prefetch::on_scan_complete();
        // Clear `current_root` so a follow-up `stopped` event doesn't
        // still carry the old root (Go doesn't include root in stopped
        // events, but the ticker path does; we keep the field for the
        // ticker to use but the stopped helper below will not include it).
        *s2.current_root.lock().await = String::new();
        log::info!("[scan] walk task finished, state=idle");
    });

    *s.running.lock().await = Some(handle);
    log::info!("[scan] start_walk_and_scan return");
    Ok(())
}

// ---------------------------------------------------------------------------
// Parallel scan engine
// ---------------------------------------------------------------------------

/// Files per fjall commit while scanning. Every commit is a journal append
/// under the journal lock; the per-file shape (which sled's writeback queue
/// used to absorb) cost ~2 commits per file and dominated the rebuild on
/// large libraries. Committing one worker batch at a time amortizes the
/// append while keeping the crash-loss window small — the scan is
/// idempotent anyway.
const SCAN_WORKER_BATCH_ROWS: usize = 512;

/// A worker's staged work, handed to the committer thread: the KV batch, the
/// rows to mirror into the tantivy media index, and the per-bucket counter
/// deltas to apply after the KV commit.
struct WorkerBatch {
    rows: Vec<MediaFile>,
    bucket_deltas: HashMap<(String, String), i64>,
    batch: crate::media::kv::Batch,
}

/// Worker threads for the walk. readdir+stat parallelize well across
/// directories; beyond ~8 the disk becomes the bottleneck, not the CPU.
fn scan_worker_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(2, 8)
}

/// Work-stealing directory queue shared by the scan workers. `active` counts
/// directories pushed but not finished, so a worker seeing an empty stack
/// with `active == 0` knows the whole tree is done (nothing new can be
/// pushed while no directory is being processed).
#[derive(Default)]
struct ScanQueue {
    stack: std::sync::Mutex<Vec<std::path::PathBuf>>,
    active: std::sync::atomic::AtomicUsize,
    cv: std::sync::Condvar,
}

impl ScanQueue {
    fn push(&self, dir: std::path::PathBuf) {
        // Increment before publishing: a concurrent `pop` must never observe
        // the stack entry while `active` still excludes it.
        self.active.fetch_add(1, Ordering::SeqCst);
        self.stack.lock().unwrap().push(dir);
        self.cv.notify_one();
    }

    fn pop(&self) -> Option<std::path::PathBuf> {
        let mut guard = self.stack.lock().unwrap();
        loop {
            if let Some(dir) = guard.pop() {
                return Some(dir);
            }
            if self.active.load(Ordering::SeqCst) == 0 {
                return None;
            }
            guard = self.cv.wait(guard).unwrap();
        }
    }

    fn dir_done(&self) {
        if self.active.fetch_sub(1, Ordering::SeqCst) == 1 {
            // Last in-flight directory finished: wake all idle workers so
            // they observe active == 0 and exit.
            self.cv.notify_all();
        }
    }
}

/// Walk `root` and index every regular file. N worker threads handle
/// readdir/stat/uuid/JSON **and** stage their rows into per-worker batches;
/// the calling thread is the single committer: it applies the KV batch
/// (one journal commit per ~512 files), mirrors the rows into the tantivy
/// media search index and updates the bucket counters. Honors pause
/// (workers sleep) and stop (flush staged rows, exit). Returns
/// `(files_seen, indexed)`.
fn scan_tree(db: &Arc<crate::media::kv::Db>, root: &Path, s: &Arc<Scanner>) -> (i64, i64) {
    let threads = scan_worker_threads();

    let (tx, rx) = std::sync::mpsc::channel::<WorkerBatch>();
    let queue = Arc::new(ScanQueue::default());

    // `rebuildMediaIndex(root)` can legitimately name a single file; a
    // symlinked root is never traversed (matches the old walker).
    match std::fs::symlink_metadata(root) {
        Ok(meta) if meta.is_file() => {
            let p = root.to_string_lossy().to_string();
            if is_media_excluded(&p) {
                return (0, 0);
            }
            return match build_scanned(db, &p, &meta) {
                Ok(row) => {
                    let mut batch = db.batch();
                    stage_scanned(&mut batch, &row);
                    match db.apply_batch(batch) {
                        Ok(()) => {
                            apply_bucket_deltas(db, &bucket_deltas_of(std::slice::from_ref(&row)));
                            let _ = crate::media::image_index::global().index_media_file(&row.m);
                            s.commits.fetch_add(1, Ordering::SeqCst);
                            s.last_indexed.store(1, Ordering::SeqCst);
                            (1, 1)
                        }
                        Err(e) => {
                            log::error!("[scan] batch commit failed: {e}");
                            (0, 0)
                        }
                    }
                }
                Err(e) => {
                    log::debug!("scan_file({p}) failed: {e}");
                    (0, 0)
                }
            };
        }
        Ok(meta) if !meta.is_dir() => return (0, 0),
        _ => {}
    }
    queue.push(root.to_path_buf());

    let mut workers = Vec::with_capacity(threads);
    for _ in 0..threads {
        let tx = tx.clone();
        let queue = queue.clone();
        let db = db.clone();
        let s = s.clone();
        workers.push(std::thread::spawn(move || scan_worker(db, queue, tx, s)));
    }
    drop(tx); // the workers hold the remaining senders

    let media_idx = crate::media::image_index::global();
    let mut files_seen: i64 = 0;
    // Iterating the receiver ends when every worker has dropped its sender,
    // i.e. the tree is walked (or stopped).
    for wb in rx {
        if let Err(e) = db.apply_batch(wb.batch) {
            log::error!("[scan] batch commit failed: {e}");
            continue;
        }
        s.commits.fetch_add(1, Ordering::SeqCst);
        // Only after the KV (source of truth) commit: derived stores.
        apply_bucket_deltas(db, &wb.bucket_deltas);
        for row in &wb.rows {
            if let Err(e) = media_idx.add_media_file(row) {
                log::error!("[scan] media index add failed: {e}");
            }
        }
        if let Err(e) = media_idx.commit() {
            log::error!("[scan] media index commit failed: {e}");
        }
        files_seen += wb.rows.len() as i64;
    }
    workers_join(workers);
    (files_seen, files_seen)
}

/// Join scan worker threads, tolerating panics (a panicked worker must not
/// take the scan task down before its partial work is accounted).
fn workers_join(workers: Vec<std::thread::JoinHandle<()>>) {
    for w in workers {
        let _ = w.join();
    }
}

/// One bucket (directory) of a media type: `mediaBuckets` GraphQL shape.
pub struct MediaBucketInfo {
    /// Directory path (the bucket id).
    pub dir: String,
    pub item_count: i64,
}

/// List bucket counters for a media type, heaviest directory first.
pub fn list_buckets(db: &crate::media::kv::Db, media_type: &str) -> Result<Vec<MediaBucketInfo>> {
    let prefix = format!("{}{media_type}:", crate::media::image_index::BUCKET_PREFIX);
    let mut out = Vec::new();
    for kv in db.scan_prefix(&prefix) {
        let (k, v) = kv?;
        let key = String::from_utf8_lossy(&k).to_string();
        let dir = key.strip_prefix(&prefix).unwrap_or_default().to_string();
        let count = String::from_utf8_lossy(&v).parse::<i64>().unwrap_or(0);
        if !dir.is_empty() && count > 0 {
            out.push(MediaBucketInfo {
                dir,
                item_count: count,
            });
        }
    }
    out.sort_by(|a, b| {
        b.item_count
            .cmp(&a.item_count)
            .then_with(|| a.dir.cmp(&b.dir))
    });
    Ok(out)
}

/// One scan worker: drain directories off the queue, stat each entry
/// (readdir's d_type classifies dirs without a stat; files need one fstatat
/// for size/mtime/inode/ctime), build and stage fully resolved rows into a
/// local KV batch, hand full batches to the committer thread together with
/// the rows (for the tantivy mirror) and the bucket counter deltas.
fn scan_worker(
    db: Arc<crate::media::kv::Db>,
    queue: Arc<ScanQueue>,
    tx: std::sync::mpsc::Sender<WorkerBatch>,
    s: Arc<Scanner>,
) {
    let mut batch = db.batch_with_capacity(SCAN_WORKER_BATCH_ROWS * 4);
    let mut rows: Vec<MediaFile> = Vec::with_capacity(SCAN_WORKER_BATCH_ROWS);
    let mut deltas: HashMap<(String, String), i64> = HashMap::new();
    // let-chains (edition 2024): a stopped worker exits without draining the
    // remaining queue — stopping must not walk the whole tree doing no-op
    // read_dirs.
    'walk: while !s.is_stopping()
        && let Some(dir) = queue.pop()
    {
        let entries = match std::fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(_) => {
                queue.dir_done();
                continue;
            }
        };
        for entry in entries.flatten() {
            if s.is_stopping() {
                break;
            }
            // Busy-wait while paused (mirrors Go's `time.Sleep(200ms)` loop);
            // stop breaks out of the pause too.
            while s.is_paused() && !s.is_stopping() {
                std::thread::sleep(Duration::from_millis(200));
            }
            if s.is_stopping() {
                break;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if !is_media_excluded(&entry.path().to_string_lossy()) {
                    queue.push(entry.path());
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            // DirEntry::metadata is a dirfd-relative fstatat (no symlink
            // follow), cheaper than resolving the full path again.
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            let path = entry.path().to_string_lossy().to_string();
            if is_media_excluded(&path) {
                continue;
            }
            match build_scanned(&db, &path, &meta) {
                Ok(row) => {
                    stage_scanned(&mut batch, &row);
                    for (k, d) in bucket_deltas_of(std::slice::from_ref(&row)) {
                        *deltas.entry(k).or_insert(0) += d;
                    }
                    rows.push(row.m);
                    s.last_indexed.fetch_add(1, Ordering::SeqCst);
                    if rows.len() == SCAN_WORKER_BATCH_ROWS {
                        let wb = WorkerBatch {
                            rows: std::mem::take(&mut rows),
                            bucket_deltas: std::mem::take(&mut deltas),
                            batch: std::mem::replace(
                                &mut batch,
                                db.batch_with_capacity(SCAN_WORKER_BATCH_ROWS * 4),
                            ),
                        };
                        if tx.send(wb).is_err() {
                            // Committer is gone; stop this worker.
                            break 'walk;
                        }
                    }
                }
                Err(e) => log::debug!("scan_file({path}) failed: {e}"),
            }
        }
        queue.dir_done();
    }
    // Flush whatever is staged (also on stop: rows already staged stay
    // committed so the index never ends up half-written for a file).
    if !rows.is_empty() {
        let _ = tx.send(WorkerBatch {
            rows,
            bucket_deltas: deltas,
            batch,
        });
    }
}

fn count_files(root: &Path) -> i64 {
    // Precount pass: classification via readdir's d_type (no per-entry stat
    // on filesystems that provide it — ext4 does), iterative so pathological
    // depth cannot overflow the stack.
    let Ok(meta) = std::fs::symlink_metadata(root) else {
        return 0;
    };
    if !meta.is_dir() {
        let p = root.to_string_lossy().to_string();
        return i64::from(meta.is_file() && !is_media_excluded(&p));
    }
    let mut n: i64 = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            // Same exclusions as the scan walk, so `total` matches what the
            // scan will actually index (otherwise progress never completes).
            let path = entry.path();
            if is_media_excluded(&path.to_string_lossy()) {
                continue;
            }
            match entry.file_type() {
                Ok(ft) if ft.is_dir() => stack.push(path),
                Ok(ft) if ft.is_file() => n += 1,
                _ => {}
            }
        }
    }
    n
}

/// Normalise to forward slashes (Go's `filepath.ToSlash`). Empty input
/// stays empty.
fn path_to_slash(p: &Path) -> String {
    if p.as_os_str().is_empty() {
        return String::new();
    }
    let s = p.to_string_lossy().to_string();
    s.replace('\\', "/")
}

/// Build and publish the `media:scan:progress` event from the current
/// scanner state. Includes the `root` field whenever one is set (Go
/// includes it in every per-second tick and in the final event).
fn publish_progress(s: &Scanner) {
    let (i, t, state) = s.get_progress();
    let pending = (t - i).max(0);
    // `root` is present in every event the *ticker* pushes (Go includes
    // it unconditionally while a scan is running). We only have an
    // empty string after the scan ends; in that case the `state` will
    // be `IDLE` and the field is omitted to keep parity with the Go
    // side's final-event behaviour (Go always sends `root` because it
    // is captured by the closure — the Rust field is cleared for the
    // same effect).
    let mut payload = serde_json::json!({
        "indexed": i, "pending": pending, "total": t, "state": state.as_str(),
    });
    if let Some(root) = s.current_root.try_lock().ok().filter(|r| !r.is_empty()) {
        payload["root"] = serde_json::Value::String(root.clone());
    }
    log::info!(
        "[scan] publish_progress indexed={} pending={} total={} state={} root_present={}",
        i,
        pending,
        t,
        state.as_str(),
        payload.get("root").is_some()
    );
    let _ = crate::media::eventbus::Bus::new().publish(crate::media::eventbus::EVENT_MEDIA_SCAN_PROGRESS, payload);
}

/// Publish the initial `{indexed:0, pending:0, total:0, state:"RUNNING",
/// root}` event for `rebuildMediaIndex`. Mirrors the synchronous publish
/// that Go's `rebuildMediaIndex` performs before spawning the scan
/// goroutine. The `root` field is the only way the frontend learns what
/// directory is being reindexed; without it, the spinner would show but
/// the path label would stay empty.
pub fn publish_initial_running(root: &Path) {
    // Make sure the scanner reflects the new state so the event we push
    // is consistent (state: "RUNNING", indexed/total: 0).
    let s = scanner();
    s.state.store(STATE_RUNNING, Ordering::SeqCst);
    s.stop_flag.store(0, Ordering::SeqCst);
    s.last_indexed.store(0, Ordering::SeqCst);
    s.last_total.store(0, Ordering::SeqCst);
    // Best-effort sync write of root — the scan loop will overwrite it
    // when it actually starts, but the initial event below must carry
    // it now.
    if let Ok(mut g) = s.current_root.try_lock() {
        *g = path_to_slash(root);
    }
    let payload = serde_json::json!({
        "indexed": 0, "pending": 0, "total": 0, "state": ScanState::Running.as_str(),
        "root": path_to_slash(root),
    });
    log::info!(
        "[scan] publish_initial_running root={}",
        path_to_slash(root)
    );
    let _ = crate::media::eventbus::Bus::new().publish(crate::media::eventbus::EVENT_MEDIA_SCAN_PROGRESS, payload);
}

/// Publish a `{... state: "<new state>"}` event in response to a
/// pause / resume that the user triggered from the UI. The Go side
/// does not do this explicitly — it relies on the per-second ticker
/// to pick up the new state on its next tick. The Rust port emits it
/// synchronously so the UI reflects the change without a 1-second lag.
pub fn publish_state_event() {
    publish_progress(&scanner());
}

/// Publish a `{... state: "STOPPED"}` event in response to the
/// `stopMediaScan` mutation. Mirrors the publish in Go's
/// `stopMediaScan` (`internal/graph/media_scan_api.go`). The `root`
/// field is intentionally **omitted** for the stopped event, matching
/// the Go side which also does not include it in this path.
pub fn publish_stopped_event() {
    let s = scanner();
    let (i, t, _) = s.get_progress();
    let pending = (t - i).max(0);
    let _ = crate::media::eventbus::Bus::new().publish(
        crate::media::eventbus::EVENT_MEDIA_SCAN_PROGRESS,
        serde_json::json!({
            "indexed": i, "pending": pending, "total": t, "state": ScanState::Stopped.as_str(),
        }),
    );
}

#[cfg(test)]
#[path = "../../tests/unit/media/scan.rs"]
mod tests;
