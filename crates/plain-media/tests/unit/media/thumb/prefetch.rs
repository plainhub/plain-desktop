//! Unit tests for `src/media/thumb_engine/prefetch.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

/// A private instance per test: full worker behavior without global
/// state leaking between tests. Its `db` slot stays unset, so `collect`
/// never replaces the manually enqueued queue.
struct TestRig {
    p: Arc<Prefetch>,
}

impl TestRig {
    fn new(rate: u32) -> Self {
        let p = Arc::new(Prefetch::new());
        p.per_sec.store(rate, Ordering::Relaxed);
        p.spawn_worker();
        TestRig { p }
    }

    fn enqueue(&self, path: &str, w: i32, h: i32) {
        self.p.pending.lock().unwrap().push_back(ThumbJob {
            path: path.to_string(),
            w,
            h,
        });
        self.p.wake.notify_one();
    }

    async fn wait_processed(&self, n: u64, timeout: Duration) -> bool {
        let deadline = tokio::time::Instant::now() + timeout;
        while self.p.processed.load(Ordering::Relaxed) < n {
            if tokio::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        true
    }
}

fn write_jpeg(path: &std::path::Path, w: u32, h: u32) {
    let img = image::RgbImage::from_fn(w, h, |x, y| {
        image::Rgb([(x % 200) as u8, (y % 200) as u8, 90])
    });
    img.save_with_format(path, image::ImageFormat::Jpeg)
        .unwrap();
}

fn cache_of(path: &std::path::Path, w: u32, h: u32) -> std::path::PathBuf {
    let meta = std::fs::metadata(path).unwrap();
    let mod_unix = meta
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    super::super::thumb_cache_path(
        &crate::media::paths::detect().cache_dir,
        path.to_str().unwrap(),
        w,
        h,
        THUMB_QUALITY as u8,
        mod_unix,
        meta.len() as i64,
    )
}

#[tokio::test]
async fn worker_generates_cache_with_ui_params() {
    let rig = TestRig::new(20);
    let dir = tempfile::tempdir().unwrap();
    let img = dir.path().join("grid.jpg");
    write_jpeg(&img, 700, 500);

    // Exactly the params BucketThumb.vue requests.
    rig.enqueue(img.to_str().unwrap(), BUCKET_THUMB_W, BUCKET_THUMB_H);
    assert!(
        rig.wait_processed(1, Duration::from_secs(10)).await,
        "job was not processed"
    );

    let cp = cache_of(&img, 128, 128);
    assert!(cp.exists(), "cache file must exist at {}", cp.display());
    assert!(cp.to_str().unwrap().contains("thumbs/"));
    std::fs::remove_file(&cp).ok();
    super::super::lru::debug_clear();
}

#[tokio::test]
async fn rate_limit_paces_jobs() {
    let rig = TestRig::new(20); // 20/s → ≥50 ms per job
    let dir = tempfile::tempdir().unwrap();
    let img = dir.path().join("paced.jpg");
    write_jpeg(&img, 60, 40); // tiny: generation is fast, pacing must dominate

    let n = 5u32;
    for _ in 0..n {
        rig.enqueue(img.to_str().unwrap(), RECENT_THUMB_W, RECENT_THUMB_H);
    }
    let start = std::time::Instant::now();
    // Wait for the whole round: counter bumps happen before the pacing
    // sleep, so pacing must be measured to round completion.
    tokio::time::timeout(Duration::from_secs(10), rig.p.round_done.notified())
        .await
        .expect("round did not complete");
    let elapsed = start.elapsed();
    assert!(
        elapsed >= Duration::from_millis(u64::from(n) * 50 - 5),
        "5 jobs at 20/s finished in {elapsed:?} — pacing is broken"
    );
    std::fs::remove_file(cache_of(&img, 50, 50)).ok();
    super::super::lru::debug_clear();
}

#[tokio::test]
async fn consumption_pauses_while_scan_busy() {
    let rig = TestRig::new(20);
    let dir = tempfile::tempdir().unwrap();
    let img = dir.path().join("busy.jpg");
    write_jpeg(&img, 60, 40);

    TEST_FORCE_BUSY.store(true, Ordering::Relaxed);
    rig.enqueue(img.to_str().unwrap(), BUCKET_THUMB_W, BUCKET_THUMB_H);
    // Give the worker ample time to (wrongly) process.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        rig.p.processed.load(Ordering::Relaxed),
        0,
        "worker must not consume while a scan is running"
    );

    TEST_FORCE_BUSY.store(false, Ordering::Relaxed);
    assert!(rig.wait_processed(1, Duration::from_secs(10)).await);
    std::fs::remove_file(cache_of(&img, 128, 128)).ok();
    super::super::lru::debug_clear();
}

#[tokio::test]
async fn disabled_prefetcher_stays_silent() {
    // No worker is spawned for an instance that is disabled, so an
    // enqueued job is never consumed and no cache appears.
    let p = Arc::new(Prefetch::new());
    p.enabled.store(false, Ordering::Relaxed);
    let dir = tempfile::tempdir().unwrap();
    let img = dir.path().join("off.jpg");
    write_jpeg(&img, 60, 40);
    p.pending.lock().unwrap().push_back(ThumbJob {
        path: img.to_str().unwrap().to_string(),
        w: BUCKET_THUMB_W,
        h: BUCKET_THUMB_H,
    });
    p.wake.notify_one();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(p.processed.load(Ordering::Relaxed), 0);
    assert!(!cache_of(&img, 128, 128).exists());

    // Global gating: init_from_config(prefetch=false) + on_scan_complete
    // produce zero background activity.
    let cfg = crate::media::config::Config::parse("[thumbnails]\nprefetch = false\n");
    let db_dir = dir.path().join("fjall");
    let db = Arc::new(crate::media::kv::Db::open(&db_dir).unwrap());
    init_from_config(
        &cfg,
        db,
        std::sync::Arc::new(crate::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap()),
    );
    let before = processed_count();
    on_scan_complete();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(processed_count(), before);
}
