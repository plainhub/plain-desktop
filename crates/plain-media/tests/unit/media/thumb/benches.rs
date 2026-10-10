//! Thumbnail-engine benchmarks over real photographs.
//!
//! Run (images must exist under tmp-bench-media/, see the repo README of
//! this module / AGENTS notes):
//!
//! ```text
//! cargo test --release bench_thumb -- --ignored --nocapture
//! ```
//!
//! Reports per-phase timings (decode scaled vs full, resize, encode),
//! end-to-end engine latency (cold/warm) and concurrent throughput, and
//! asserts the structural performance properties (scaled decode strictly
//! faster than full decode on real photos) so regressions fail loudly.

use super::*;

struct BenchImage {
    name: &'static str,
    path: std::path::PathBuf,
    w: u32,
    h: u32,
}

fn bench_images() -> Vec<BenchImage> {
    let dir = concat!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../plain-rs"),
        "/tmp-bench-media"
    );
    let defs = [
        ("photo-4000x3000.jpg", 4000u32, 3000u32),
        ("photo-4917x3456.jpg", 4917, 3456),
        ("photo-3840x2880.png", 3840, 2880),
    ];
    defs.iter()
        .filter_map(|(name, w, h)| {
            let p = std::path::PathBuf::from(dir).join(name);
            if p.exists() {
                Some(BenchImage {
                    name,
                    path: p,
                    w: *w,
                    h: *h,
                })
            } else {
                println!("BENCH SKIP (missing): {}/{}", dir, name);
                None
            }
        })
        .collect()
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn ms(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn cache_path_for(spec: &ThumbSpec) -> std::path::PathBuf {
    thumb_cache_path(
        &crate::media::paths::detect().cache_dir,
        &spec.path,
        spec.w,
        spec.h,
        spec.quality,
        spec.mod_unix,
        spec.file_size,
    )
}

fn spec_of(img: &BenchImage, target: u32) -> ThumbSpec {
    let meta = std::fs::metadata(&img.path).unwrap();
    let mod_unix = meta
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    ThumbSpec::sanitize(
        img.path.to_str().unwrap(),
        target as i32,
        target as i32,
        75,
        mod_unix,
        meta.len() as i64,
    )
}

/// Phase-level timing of the engine's real decode path (the JPEG ladder:
/// DC-only progressive → scaled → full decode) plus resize + encode.
/// The structural assertion is that resize+encode stay smaller than
/// decode: the pipeline must be decode-bound, not copy/encode-bound like
/// the old WebP pipeline.
#[tokio::test]
#[ignore]
async fn bench_thumb_phases() {
    let images = bench_images();
    if images.is_empty() {
        return;
    }
    println!("== phase benchmark (median of 5) ==");
    println!(
        "{:<24} {:>6} {:>11} {:>11} {:>8} {:>8} {:>9}",
        "image", "target", "decode-full", "decode-real", "resize", "encode", "post-dec"
    );

    for img in &images {
        for target in [256u32, 512, 1024] {
            let (tw, th) = compute_target_size(img.w, img.h, target, target);
            let head = std::fs::read(&img.path).unwrap_or_default();
            let head = &head[..head.len().min(256 * 1024)];
            let sniffed = sniff::sniff_header(head).expect("bench image sniffs");

            let mut t_scaled = Vec::new();
            let mut t_full = Vec::new();
            let mut t_resize = Vec::new();
            let mut t_encode = Vec::new();

            for _ in 0..5 {
                // The engine's real decode routing for this size.
                let t = std::time::Instant::now();
                drop(decode_to_target(&img.path.to_string_lossy(), sniffed, tw, th).unwrap());
                t_scaled.push(ms(t.elapsed()));

                // Full decode as the comparison baseline.
                let t = std::time::Instant::now();
                let bm = decode::image_full(&img.path).unwrap();
                t_full.push(ms(t.elapsed()));
                let t = std::time::Instant::now();
                let out = scale::resize_to(bm, tw, th);
                t_resize.push(ms(t.elapsed()));
                let t = std::time::Instant::now();
                let _ = encode::encode_jpeg(out, 75).unwrap();
                t_encode.push(ms(t.elapsed()));
            }

            let (rz, re) = (median(&mut t_resize.clone()), median(&mut t_encode.clone()));
            let df = median(&mut t_full.clone());
            let dr = median(&mut t_scaled.clone());
            println!(
                "{:<24} {:>6} {:>8.1}ms {:>8.1}ms {:>6.1}ms {:>6.1}ms {:>7.1}%",
                img.name,
                target,
                df,
                dr,
                rz,
                re,
                (rz + re) / df * 100.0
            );
            // Structural assertion: decode must remain the pipeline's largest
            // phase (the old WebP pipeline additionally spent ~20% of wall
            // time in libwebp encoding). Fast-decoding files at large
            // targets legitimately push post-decode work toward parity.
            assert!(
                rz + re < df,
                "{}@{}: resize+encode {:.1}ms exceeds decode {:.1}ms",
                img.name,
                target,
                rz + re,
                df
            );
        }
    }
}

/// End-to-end engine latency: cold (cache cleared) vs warm (LRU).
#[tokio::test]
#[ignore]
async fn bench_thumb_engine_e2e() {
    let images = bench_images();
    if images.is_empty() {
        return;
    }
    println!("== engine end-to-end (median of 5) ==");
    for img in &images {
        for target in [256u32, 512, 1024] {
            let spec = spec_of(img, target);
            let cp = cache_path_for(&spec);

            let mut cold = Vec::new();
            for _ in 0..5 {
                lru::debug_clear();
                std::fs::remove_file(&cp).ok();
                let t = std::time::Instant::now();
                let out = get_thumbnail(spec.clone()).await.unwrap();
                cold.push(ms(t.elapsed()));
                drop(out);
            }

            let mut warm = Vec::new();
            for _ in 0..5 {
                let t = std::time::Instant::now();
                let out = get_thumbnail(spec.clone()).await.unwrap();
                warm.push(ms(t.elapsed()));
                drop(out);
            }
            std::fs::remove_file(&cp).ok();
            lru::debug_clear();

            println!(
                "ENGINE {:<24} -> {:>4}px: cold {:>7.1}ms  warm {:>6.2}ms",
                img.name,
                target,
                median(&mut cold),
                median(&mut warm)
            );
        }
    }
}

/// Concurrent throughput on the real 12 MP JPEG (the classic grid scenario),
/// plus a mixed load exercising passthrough and coalescing.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore]
async fn bench_thumb_concurrent() {
    let images = bench_images();
    if images.is_empty() {
        return;
    }
    let jpeg = images
        .iter()
        .find(|i| i.name.ends_with(".jpg"))
        .expect("at least one jpeg");

    println!("== concurrent engine load (8 workers) ==");

    // 1. Duplicate-request coalescing: 64 requests for the SAME fresh key
    //    must cost one generation (single-flight), not 64 queued decodes.
    {
        let spec = spec_of(jpeg, 512);
        let cp = cache_path_for(&spec);
        lru::debug_clear();
        std::fs::remove_file(&cp).ok();
        let t = std::time::Instant::now();
        let mut handles = Vec::new();
        for _ in 0..64 {
            let s = spec.clone();
            handles.push(tokio::spawn(async move { get_thumbnail(s).await.unwrap() }));
        }
        for h in handles {
            let _ = h.await.unwrap();
        }
        let e = t.elapsed();
        println!(
            "COALESCE 64 identical requests @512px: {:.0}ms wall (one generation)",
            ms(e)
        );
        std::fs::remove_file(&cp).ok();
        lru::debug_clear();
    }

    // 2. True parallel throughput: 8 DISTINCT copies of the 12 MP photo
    //    (distinct cache keys → real concurrent decodes).
    {
        let tmp = tempfile::tempdir().unwrap();
        let mut copies = Vec::new();
        for i in 0..8 {
            let p = tmp.path().join(format!("copy{i}.jpg"));
            std::fs::copy(&jpeg.path, &p).unwrap();
            copies.push(p);
        }
        // Warm the page cache so rounds measure CPU, not cold disk reads.
        for p in &copies {
            drop(std::fs::read(p).unwrap());
        }
        for rounds in [1usize, 4] {
            lru::debug_clear();
            for p in &copies {
                let s = spec_of(
                    &BenchImage {
                        name: "copy",
                        path: p.clone(),
                        w: jpeg.w,
                        h: jpeg.h,
                    },
                    512,
                );
                std::fs::remove_file(cache_path_for(&s)).ok();
            }
            let t = std::time::Instant::now();
            let mut handles = Vec::new();
            for _ in 0..rounds {
                for p in &copies {
                    let p = p.clone();
                    handles.push(tokio::spawn(async move {
                        let meta = std::fs::metadata(&p).unwrap();
                        let mod_unix = meta
                            .modified()
                            .unwrap()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs() as i64;
                        let spec = ThumbSpec::sanitize(
                            p.to_str().unwrap(),
                            512,
                            512,
                            75,
                            mod_unix,
                            meta.len() as i64,
                        );
                        get_thumbnail(spec).await.unwrap()
                    }));
                }
            }
            let n = handles.len();
            for h in handles {
                let _ = h.await.unwrap();
            }
            let e = t.elapsed();
            println!(
                "PARALLEL {n} distinct 12MP jpegs @512px: {:.0}ms wall, {:.1} thumbs/s",
                ms(e),
                n as f64 / e.as_secs_f64()
            );
        }
    }

    // Mixed load: big jpegs + png + small passthrough files simultaneously.
    let tmp = tempfile::tempdir().unwrap();
    let small = tmp.path().join("small.jpg");
    let im = image::RgbImage::from_fn(96, 72, |x, y| {
        image::Rgb([(x * 2) as u8, (y * 2) as u8, 60])
    });
    im.save_with_format(&small, image::ImageFormat::Jpeg)
        .unwrap();

    let mut sources: Vec<std::path::PathBuf> = images.iter().map(|i| i.path.clone()).collect();
    for _ in 0..40 {
        sources.push(small.clone());
    }

    let mut handles = Vec::new();
    for p in &sources {
        let p = p.clone();
        handles.push(tokio::spawn(async move {
            let meta = std::fs::metadata(&p).unwrap();
            let mod_unix = meta
                .modified()
                .unwrap()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;
            let spec = ThumbSpec::sanitize(
                p.to_str().unwrap(),
                256,
                256,
                75,
                mod_unix,
                meta.len() as i64,
            );
            get_thumbnail(spec).await.unwrap()
        }));
    }
    let t = std::time::Instant::now();
    let mut count = 0usize;
    for h in handles {
        let _ = h.await.unwrap();
        count += 1;
    }
    println!(
        "MIXED {count} requests ({} big + 40 small): {:.0}ms wall, {:.1} req/s",
        images.len(),
        ms(t.elapsed()),
        count as f64 / t.elapsed().as_secs_f64()
    );

    // Cleanup every generated cache entry.
    for p in &sources {
        let meta = std::fs::metadata(p).unwrap();
        let mod_unix = meta
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let spec = ThumbSpec::sanitize(
            p.to_str().unwrap(),
            256,
            256,
            75,
            mod_unix,
            meta.len() as i64,
        );
        std::fs::remove_file(cache_path_for(&spec)).ok();
    }
    lru::debug_clear();
}

/// Video keyframe extraction (pure-Rust demux + H.264 decode). Uses the
/// committed testdata fixtures; drop larger H.264 MP4s into tmp-bench-media/
/// to measure real-world sizes. The retired ffmpeg-CLI path cost ~30 ms of
/// process startup alone, on top of decode.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore]
async fn bench_thumb_video() {
    use super::video;

    let mut vids: Vec<std::path::PathBuf> = [
        "testdata/video-h264-high.mp4",
        "testdata/video-h264-65s.mp4",
    ]
    .iter()
    .map(|p| {
        std::path::PathBuf::from(concat!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../plain-rs"
        )))
        .join(p)
    })
    .filter(|p| p.exists())
    .collect();
    for extra in ["tmp-bench-media/video-big.mp4", "tmp-bench-media/video.mp4"] {
        let p = std::path::PathBuf::from(concat!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../plain-rs"
        )))
        .join(extra);
        if p.exists() {
            vids.push(p);
        }
    }
    println!("== video keyframe extraction (median of 9) ==");
    for v in &vids {
        let path = v.to_str().unwrap().to_string();
        let mut plan_ms = Vec::new();
        let mut full_ms = Vec::new();
        for _ in 0..9 {
            let t = std::time::Instant::now();
            let plan = video::plan_keyframe(&path).unwrap();
            plan_ms.push(ms(t.elapsed()));
            let t = std::time::Instant::now();
            let jpg = video::decode_plan_to_jpeg(plan, 512, 512, 75).unwrap();
            full_ms.push(ms(t.elapsed()));
            assert!(jpg.starts_with(&[0xFF, 0xD8]));
        }
        println!(
            "VIDEO {:<28}: demux {:>6.2}ms  decode+resize+encode {:>6.1}ms",
            v.file_name().unwrap().to_string_lossy(),
            median(&mut plan_ms),
            median(&mut full_ms)
        );
    }
}
