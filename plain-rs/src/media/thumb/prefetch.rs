//! Background thumbnail prefetcher.
//!
//! The engine makes *repeat* requests sub-millisecond (LRU + file cache +
//! single-flight), but the first page-open still cold-generates. The grid
//! knows in advance which files it will ask for — `mediaBuckets` topItems
//! (4 per directory) and `db::recent` — so a low-rate worker pre-generates
//! those in the background and turns first-open latency into a cache hit.
//!
//! Sizing mirrors exactly what the web UI requests (grep of the shared
//! frontend): bucket grids ask `/fs?w=128&h=128` (`BucketThumb.vue`), the
//! files page's recent list asks `w=50&h=50` (`lib/file.ts fileThumbUrl`),
//! both with the default quality 75. A cache key that differs in any of
//! w/h/q/mtime/size is a miss, so these constants must track the frontend.
//!
//! Behavior: a single worker consumes a pending queue at a bounded rate
//! (default 2 thumbs/s, `[thumbnails] prefetch_per_sec`, clamped 1..=20).
//! The queue is (re)collected at startup, on scan completion, and on a
//! 10-minute idle ticker; a fresh collect replaces stale pending jobs.
//! While a media scan is running, consumption pauses (the scan owns the
//! disk). Everything goes through the engine's public `get_thumbnail`
//! entry, so admission control and cache-skip apply unchanged — the worker
//! adds no parallelism of its own beyond its single task + low rate.
//! Failed paths are dropped for the round (try-once; the next collect
//! re-enqueues them).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use super::ThumbSpec;

/// Bucket-grid box the UI requests (`BucketThumb.vue`).
const BUCKET_THUMB_W: i32 = 128;
const BUCKET_THUMB_H: i32 = 128;
/// Recent-list box the UI requests (`lib/file.ts fileThumbUrl`).
const RECENT_THUMB_W: i32 = 50;
const RECENT_THUMB_H: i32 = 50;
/// UI requests use the default quality (no `q=` param).
const THUMB_QUALITY: i32 = 75;
/// Cap on enqueued jobs per round (buckets of a very large library).
const MAX_JOBS: usize = 2000;
/// How many recent files to warm.
const RECENT_LIMIT: usize = 50;
/// Idle re-collect period.
const TICK_PERIOD: Duration = Duration::from_secs(600);

/// One pre-warm target: source path + the exact box the UI will request.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ThumbJob {
    path: String,
    w: i32,
    h: i32,
}

pub struct Prefetch {
    enabled: AtomicBool,
    per_sec: AtomicU32,
    pending: Mutex<VecDeque<ThumbJob>>,
    wake: tokio::sync::Notify,
    /// Fires when a drain round that processed ≥1 job completes (tests pace
    /// whole rounds, not counter bumps which happen before the pacing sleep).
    round_done: tokio::sync::Notify,
    /// Jobs processed since start (tests + the zero-activity assertion for
    /// `prefetch = false`).
    processed: AtomicU64,
    db: OnceLock<Arc<crate::media::kv::Db>>,
    prefs: OnceLock<Arc<crate::prefs::Prefs>>,
}

impl Prefetch {
    fn new() -> Self {
        Prefetch {
            enabled: AtomicBool::new(true),
            per_sec: AtomicU32::new(2),
            pending: Mutex::new(VecDeque::new()),
            wake: tokio::sync::Notify::new(),
            round_done: tokio::sync::Notify::new(),
            processed: AtomicU64::new(0),
            db: OnceLock::new(),
            prefs: OnceLock::new(),
        }
    }

    /// Worker: idle-collect on a ticker plus wakeups from scan completion;
    /// each collect drains to completion at the configured rate.
    fn spawn_worker(self: &Arc<Self>) {
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(TICK_PERIOD);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            ticker.tick().await; // the first interval tick fires immediately
            loop {
                tokio::select! {
                    _ = ticker.tick() => {}
                    _ = this.wake.notified() => {}
                }
                this.collect().await;
                this.drain().await;
            }
        });
    }

    /// Gather the current prefetch set (bucket topItems for every media
    /// type + recent files) and replace the pending queue. Without a DB
    /// handle (tests driving the queue directly) this is a no-op that
    /// preserves the manually enqueued jobs.
    async fn collect(&self) {
        let (Some(db), Some(prefs)) = (self.db.get().cloned(), self.prefs.get().cloned()) else {
            return;
        };
        let jobs = tokio::task::spawn_blocking(move || collect_jobs(&db, &prefs))
            .await
            .unwrap_or_default();
        let n = jobs.len();
        *self.pending.lock().unwrap() = VecDeque::from(jobs);
        if n > 0 {
            log::info!("[prefetch] queued {n} thumbnails (bucket topItems + recent)");
        }
    }

    /// Process queued jobs one by one, rate-limited, pausing while a media
    /// scan is running. Errors are dropped (try-once per round).
    async fn drain(&self) {
        let per_sec = self.per_sec.load(Ordering::Relaxed).clamp(1, 20);
        let min_gap = Duration::from_secs(1) / per_sec;
        let round_start = std::time::Instant::now();
        let mut index: u32 = 0;
        loop {
            let Some(job) = self.pending.lock().unwrap().pop_front() else {
                break;
            };
            // The scan owns the disks: hold off until it is done.
            while scan_is_busy() {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            // Absolute pacing: job `i` (0-based) of a round may not finish
            // before round_start + (i+1)·gap, however cheap it was.
            index += 1;
            if let Ok(meta) = tokio::fs::metadata(&job.path).await {
                let mod_unix = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                let spec = ThumbSpec::sanitize(
                    &job.path,
                    job.w,
                    job.h,
                    THUMB_QUALITY,
                    mod_unix,
                    meta.len() as i64,
                );
                let _ = super::get_thumbnail(spec).await;
                self.processed.fetch_add(1, Ordering::Relaxed);
            }
            let target = round_start + min_gap * index;
            let now = std::time::Instant::now();
            if now < target {
                tokio::time::sleep(target - now).await;
            }
        }
        if index > 0 {
            self.round_done.notify_one();
        }
    }
}

/// Build the full prefetch list. Query cost tracks the buckets themselves
/// (one small index query per directory), capped to [`MAX_JOBS`] entries.
fn collect_jobs(db: &crate::media::kv::Db, prefs: &crate::prefs::Prefs) -> Vec<ThumbJob> {
    let mut jobs: Vec<ThumbJob> = Vec::new();
    let mut seen: std::collections::HashSet<(String, i32, i32)> = std::collections::HashSet::new();

    for kind in ["image", "video", "audio"] {
        if jobs.len() >= MAX_JOBS {
            break;
        }
        let Ok(buckets) = crate::media::scan::list_buckets(db, kind) else {
            continue;
        };
        let wanted: std::collections::HashSet<String> =
            buckets.iter().map(|b| b.dir.clone()).collect();
        let Ok(tops) = crate::media::image_index::global().bucket_top_items(kind, &wanted, 4)
        else {
            continue;
        };
        for dir in &wanted {
            if let Some(paths) = tops.get(dir) {
                for p in paths {
                    push_job(&mut jobs, &mut seen, p, BUCKET_THUMB_W, BUCKET_THUMB_H);
                }
            }
        }
    }
    for p in crate::media::kv::recent::get_recent_files(prefs, RECENT_LIMIT) {
        push_job(&mut jobs, &mut seen, &p, RECENT_THUMB_W, RECENT_THUMB_H);
    }
    jobs.truncate(MAX_JOBS);
    jobs
}

fn push_job(
    jobs: &mut Vec<ThumbJob>,
    seen: &mut std::collections::HashSet<(String, i32, i32)>,
    path: &str,
    w: i32,
    h: i32,
) {
    if !path.is_empty() && seen.insert((path.to_string(), w, h)) {
        jobs.push(ThumbJob {
            path: path.to_string(),
            w,
            h,
        });
    }
}

/// A media scan holds the disks; the worker waits it out. Test-overridable.
fn scan_is_busy() -> bool {
    #[cfg(test)]
    if TEST_FORCE_BUSY.load(Ordering::Relaxed) {
        return true;
    }
    matches!(
        crate::media::scan::scanner().state(),
        crate::media::scan::ScanState::Running | crate::media::scan::ScanState::Paused
    )
}

#[cfg(test)]
static TEST_FORCE_BUSY: AtomicBool = AtomicBool::new(false);

static GLOBAL: std::sync::LazyLock<Arc<Prefetch>> =
    std::sync::LazyLock::new(|| Arc::new(Prefetch::new()));

/// Configure + start the global prefetcher (`[thumbnails] prefetch`,
/// `prefetch_per_sec`). Called once at startup after the DB is open; with
/// `prefetch = false` no worker is spawned and triggers are no-ops. The key
/// is absent-by-default-on, like the other `[thumbnails]` knobs.
pub fn init_from_config(
    cfg: &crate::media::config::Config,
    db: Arc<crate::media::kv::Db>,
    prefs: Arc<crate::prefs::Prefs>,
) {
    let g = &*GLOBAL;
    let _ = g.db.set(db);
    let _ = g.prefs.set(prefs);
    let raw = cfg.get_int("thumbnails.prefetch_per_sec");
    g.per_sec.store(
        if raw <= 0 { 2 } else { raw.clamp(1, 20) as u32 },
        Ordering::Relaxed,
    );
    let enabled = if cfg.get_string("thumbnails.prefetch").is_empty() {
        true
    } else {
        cfg.get_bool("thumbnails.prefetch")
    };
    g.enabled.store(enabled, Ordering::Relaxed);
    if enabled {
        log::info!(
            "[prefetch] enabled, rate {} thumbs/s",
            g.per_sec.load(Ordering::Relaxed)
        );
        g.spawn_worker();
    } else {
        log::info!("[prefetch] disabled by config");
    }
}

/// Scan-completion trigger: wake the worker, which re-collects (fresh
/// topItems) and drains. Notify is sync, cheap and non-blocking for the
/// scan task.
pub fn on_scan_complete() {
    if !GLOBAL.enabled.load(Ordering::Relaxed) {
        return;
    }
    GLOBAL.wake.notify_one();
}

/// Tests / diagnostics: jobs processed since start.
pub fn processed_count() -> u64 {
    GLOBAL.processed.load(Ordering::Relaxed)
}

#[cfg(test)]
#[path = "../../../tests/unit/media/thumb/prefetch.rs"]
mod tests;
