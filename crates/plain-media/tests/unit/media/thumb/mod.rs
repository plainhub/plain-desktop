//! Unit tests for `src/media/thumb_engine/mod.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

fn write_jpeg(path: &std::path::Path, w: u32, h: u32) {
    let img = image::RgbImage::from_fn(w, h, |x, y| {
        image::Rgb([(x % 200) as u8, (y % 200) as u8, 90])
    });
    img.save_with_format(path, image::ImageFormat::Jpeg)
        .unwrap();
}

fn spec_for(path: &std::path::Path, w: u32, h: u32) -> ThumbSpec {
    let meta = std::fs::metadata(path).unwrap();
    let mod_unix = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    ThumbSpec::sanitize(
        path.to_str().unwrap(),
        w as i32,
        h as i32,
        75,
        mod_unix,
        meta.len() as i64,
    )
}

fn fresh_dir(_tag: &str) -> tempfile::TempDir {
    // Under $HOME (macOS temp lives under /var/folders which the media
    // scan would exclude; irrelevant here, but consistent with the rest
    // of the suite).
    tempfile::tempdir().unwrap()
}

#[test]
fn compute_target_size_matrix() {
    // Box fit.
    let (tw, th) = compute_target_size(800, 600, 200, 200);
    assert_eq!((tw, th), (200, 150));
    // No upscale.
    assert_eq!(compute_target_size(100, 100, 200, 200), (100, 100));
    // Width-only.
    assert_eq!(compute_target_size(800, 600, 400, 0), (400, 300));
    // Height-only.
    assert_eq!(compute_target_size(800, 600, 0, 150), (200, 150));
    // Unconstrained.
    assert_eq!(compute_target_size(800, 600, 0, 0), (800, 600));
    // Extreme aspect: never collapses to zero.
    let (tw, th) = compute_target_size(10000, 2, 100, 100);
    assert_eq!((tw, th), (100, 1));
}

#[test]
fn jpeg_decode_strategy_matrix() {
    let prog = Sniffed {
        kind: ImageKind::Jpeg,
        width: 4000,
        height: 6000,
        progressive: true,
    };
    let base = Sniffed {
        progressive: false,
        ..prog
    };
    // Big-to-small: scaled decode (either-axis ratio >= 2).
    assert!(jpeg_use_scaled(4000, 3000, 256, 256));
    assert!(jpeg_use_scaled(4000, 3000, 2000, 1500));
    assert!(!jpeg_use_scaled(4000, 3000, 3000, 2000));
    // Progressive + 1/8 grid covers the actual target: DC-only fast path.
    // (A 512 box on 4000x6000 computes a 341x512 target; 1/8 = 500x750.)
    assert!(dc_fast_eligible(prog, 341, 512));
    assert!(dc_fast_eligible(prog, 128, 128));
    // Full 512x512 target exceeds the 1/8 width (500) → scaled decoder.
    assert!(!dc_fast_eligible(prog, 512, 512));
    // 1024px needs more than the 1/8 grid → scaled decoder.
    assert!(!dc_fast_eligible(prog, 1024, 1024));
    // Baseline never takes the DC path.
    assert!(!dc_fast_eligible(base, 128, 128));
    // Small progressive files (1/8 grid below target) neither.
    let small_prog = Sniffed {
        kind: ImageKind::Jpeg,
        width: 1536,
        height: 2288,
        progressive: true,
    };
    assert!(!dc_fast_eligible(small_prog, 512, 512)); // 192x286 < 512
    assert!(dc_fast_eligible(small_prog, 128, 128));
}

#[test]
fn spec_sanitize_clamps() {
    let s = ThumbSpec::sanitize("/x", -5, 9_999_999, 1000, 1, 2);
    assert_eq!((s.w, s.h, s.quality), (0, MAX_TARGET_DIM, 100));
}

#[test]
fn cache_path_determinism_and_invalidation() {
    let dir = std::path::Path::new("/tmp/cache");
    let a = thumb_cache_path(dir, "/a/b.jpg", 200, 200, 75, 1000, 5000);
    let b = thumb_cache_path(dir, "/a/b.jpg", 200, 200, 75, 1000, 5000);
    assert_eq!(a, b);
    assert!(a.to_str().unwrap().contains("thumbs/"));
    assert!(a.to_str().unwrap().ends_with(".jpg"));
    assert_ne!(
        a,
        thumb_cache_path(dir, "/a/b.jpg", 200, 200, 75, 1001, 5000)
    );
    assert_ne!(
        a,
        thumb_cache_path(dir, "/a/b.jpg", 256, 256, 75, 1000, 5000)
    );
    // The format marker changed from the webp era: old and new keys
    // must not collide.
    use sha1::{Digest, Sha1};
    let mut h = Sha1::new();
    h.update(b"/a/b.jpg|webp|200x200|q75|m1000|s5000");
    let old = crate::utils::hex::bytes_to_hex(&h.finalize());
    assert_ne!(
        a.file_name().unwrap().to_str().unwrap(),
        format!("{old}.webp")
    );
    let etag = cache_etag(&a);
    assert!(etag.starts_with("\"t") && etag.ends_with('"'));
    assert_eq!(etag, cache_etag(&b));
}

#[tokio::test]
async fn end_to_end_generate_then_cache_hit() {
    let dir = fresh_dir("e2e");
    let img = dir.path().join("photo.jpg");
    write_jpeg(&img, 640, 480);

    let spec = spec_for(&img, 128, 128);
    let out = get_thumbnail(spec.clone()).await.unwrap();
    let ThumbOutcome::Generated(d1) = out else {
        panic!("640x480 source must be generated, not passed through");
    };
    assert!(!d1.is_empty());
    assert!(d1.starts_with(&[0xFF, 0xD8]));
    // Cache file exists with identical bytes.
    let cp = thumb_cache_path(
        &crate::media::paths::detect().cache_dir,
        &spec.path,
        spec.w,
        spec.h,
        spec.quality,
        spec.mod_unix,
        spec.file_size,
    );
    assert!(cp.exists());
    assert_eq!(std::fs::read(&cp).unwrap(), d1.as_slice());

    // Second call: served from cache, byte-identical, no new generation.
    let key = cp.to_string_lossy().into_owned();
    let before = generations_for(&key);
    let out2 = get_thumbnail(spec.clone()).await.unwrap();
    let ThumbOutcome::Generated(d2) = out2 else {
        panic!("cached response must be Generated");
    };
    assert_eq!(d1, d2);
    assert_eq!(
        generations_for(&key),
        before,
        "cache hit must not regenerate"
    );
    std::fs::remove_file(&cp).ok();
    lru::debug_clear();
}

#[tokio::test]
async fn concurrent_identical_requests_coalesce() {
    let dir = fresh_dir("coalesce");
    let img = dir.path().join("big.jpg");
    write_jpeg(&img, 1200, 900);
    let spec = spec_for(&img, 256, 256);

    let key = thumb_cache_path(
        &crate::media::paths::detect().cache_dir,
        &spec.path,
        spec.w,
        spec.h,
        spec.quality,
        spec.mod_unix,
        spec.file_size,
    )
    .to_string_lossy()
    .into_owned();
    let before = generations_for(&key);
    let mut handles = Vec::new();
    for _ in 0..32 {
        let s = spec.clone();
        handles.push(tokio::spawn(async move { get_thumbnail(s).await.unwrap() }));
    }
    for h in handles {
        let ThumbOutcome::Generated(d) = h.await.unwrap() else {
            panic!("expected Generated");
        };
        assert!(!d.is_empty());
    }
    let after = generations_for(&key);
    assert!(
        after <= before + 1,
        "32 identical requests produced {} generations",
        after - before
    );
    // Cleanup for other tests.
    let cp = thumb_cache_path(
        &crate::media::paths::detect().cache_dir,
        &spec.path,
        spec.w,
        spec.h,
        spec.quality,
        spec.mod_unix,
        spec.file_size,
    );
    std::fs::remove_file(&cp).ok();
    lru::debug_clear();
}

#[tokio::test]
async fn passthrough_small_images() {
    let dir = fresh_dir("pass");
    // Small JPEG (fits box): bytes served as-is.
    let img = dir.path().join("small.jpg");
    write_jpeg(&img, 64, 48);
    let out = get_thumbnail(spec_for(&img, 256, 256)).await.unwrap();
    match out {
        ThumbOutcome::Original { data, mime } => {
            assert_eq!(mime, "image/jpeg");
            assert_eq!(data.as_slice(), std::fs::read(&img).unwrap());
        }
        _ => panic!("small jpeg must pass through"),
    }

    // Small PNG with alpha also passes through (alpha preserved).
    let png = dir.path().join("small.png");
    image::RgbaImage::from_fn(32, 32, |_, _| image::Rgba([1, 2, 3, 0]))
        .save_with_format(&png, image::ImageFormat::Png)
        .unwrap();
    match get_thumbnail(spec_for(&png, 64, 64)).await.unwrap() {
        ThumbOutcome::Original { data, mime } => {
            assert_eq!(mime, "image/png");
            assert_eq!(data.as_slice(), std::fs::read(&png).unwrap());
        }
        _ => panic!("small png must pass through"),
    }

    // No target dims + small file: also passthrough (cc=1&w=0&h=0).
    match get_thumbnail(spec_for(&img, 0, 0)).await.unwrap() {
        ThumbOutcome::Original { .. } => {}
        _ => panic!("no-dim request on small file must pass through"),
    }
}

#[tokio::test]
async fn exif_oriented_thumbnail_dimensions() {
    let dir = fresh_dir("exif");
    // 400x300 landscape stored with orientation 6 (rotate 90 CW to view):
    // upright is 300x400. Target 100x100 → upright thumb 75x100.
    let img = dir.path().join("rot6.jpg");
    write_jpeg(&img, 400, 300);
    let mut bytes = std::fs::read(&img).unwrap();
    let app1 = exif::test_app1_orientation(6);
    let mut with_exif = bytes[..2].to_vec();
    with_exif.extend_from_slice(&app1);
    with_exif.extend_from_slice(&bytes[2..]);
    bytes = with_exif;
    std::fs::write(&img, &bytes).unwrap();

    let spec = spec_for(&img, 100, 100);
    let out = get_thumbnail(spec.clone()).await.unwrap();
    let ThumbOutcome::Generated(d) = out else {
        panic!("must generate");
    };
    let decoded = image::load_from_memory(&d).unwrap();
    assert_eq!(
        (decoded.width(), decoded.height()),
        (75, 100),
        "orientation-6 thumb must be portrait 75x100"
    );
    let cp = thumb_cache_path(
        &crate::media::paths::detect().cache_dir,
        &spec.path,
        spec.w,
        spec.h,
        spec.quality,
        spec.mod_unix,
        spec.file_size,
    );
    std::fs::remove_file(&cp).ok();
    lru::debug_clear();
}

#[tokio::test]
async fn unicode_and_spaced_paths() {
    let dir = fresh_dir("paths");
    let sub = dir.path().join("ünïcode 空 目録");
    std::fs::create_dir_all(&sub).unwrap();
    let img = sub.join("photo name #1.JPG");
    write_jpeg(&img, 320, 240);
    let out = get_thumbnail(spec_for(&img, 64, 64)).await.unwrap();
    assert!(matches!(out, ThumbOutcome::Generated(_)));
    let img2 = dir.path().join("Ünicode 名前 test.jpg");
    write_jpeg(&img2, 320, 240);
    assert!(matches!(
        get_thumbnail(spec_for(&img2, 64, 64)).await.unwrap(),
        ThumbOutcome::Generated(_)
    ));
    // Cleanup: engine writes to the real cache dir.
    for p in [&img, &img2] {
        let s = spec_for(p, 64, 64);
        let cp = thumb_cache_path(
            &crate::media::paths::detect().cache_dir,
            &s.path,
            s.w,
            s.h,
            s.quality,
            s.mod_unix,
            s.file_size,
        );
        std::fs::remove_file(&cp).ok();
    }
    lru::debug_clear();
}

#[tokio::test]
async fn bomb_header_rejected() {
    let dir = fresh_dir("bomb");
    let fake = dir.path().join("bomb.jpg");
    // A JPEG header claiming 40000x40000 with garbage payload.
    let mut v = vec![0xFF, 0xD8];
    v.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08]);
    v.extend_from_slice(&40000u16.to_be_bytes());
    v.extend_from_slice(&40000u16.to_be_bytes());
    v.extend_from_slice(&[3, 1, 0x11, 1, 0x11, 1, 0x11]);
    v.extend_from_slice(&[0x55; 64]);
    std::fs::write(&fake, &v).unwrap();
    // Not JPEG-scalable-check first: sniff OK → guard trips (1.6G px
    // bitmap for a full decode path... ratio≥2 so scaled path is chosen
    // and est is small; decode then fails on garbage → Err either way).
    let r = get_thumbnail(spec_for(&fake, 64, 64)).await;
    assert!(r.is_err(), "garbage payload must not yield a thumbnail");

    // True bomb: PNG claiming 50000x50000 (full-decode path).
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    png.extend_from_slice(&13u32.to_be_bytes());
    png.extend_from_slice(b"IHDR");
    png.extend_from_slice(&50000u32.to_be_bytes());
    png.extend_from_slice(&50000u32.to_be_bytes());
    png.extend_from_slice(&[8, 6, 0, 0, 0]);
    png.extend_from_slice(&[0; 16]);
    let fakepng = dir.path().join("bomb.png");
    std::fs::write(&fakepng, &png).unwrap();
    let err = get_thumbnail(spec_for(&fakepng, 64, 64))
        .await
        .expect_err("50000x50000 PNG must be rejected by the pixel guard");
    assert!(err.to_string().contains("pixel guard"), "got: {err}");
}

#[tokio::test]
async fn corrupt_and_foreign_files_error() {
    let dir = fresh_dir("err");
    // Garbage with a non-image extension.
    let txt = dir.path().join("note.txt");
    std::fs::write(&txt, b"hello world 1234").unwrap();
    assert!(get_thumbnail(spec_for(&txt, 64, 64)).await.is_err());
    // Empty file with .jpg extension.
    let empty = dir.path().join("empty.jpg");
    std::fs::write(&empty, b"").unwrap();
    assert!(get_thumbnail(spec_for(&empty, 64, 64)).await.is_err());
    // Truncated real JPEG (header ok, body cut): decoders are lenient —
    // zune fills the missing scan data, so a thumbnail may still come
    // out (same behavior as the previous pipeline). The contract is
    // "never panic, output is always a decodable JPEG when Ok".
    let trunc = dir.path().join("trunc.jpg");
    write_jpeg(&trunc, 200, 150);
    let bytes = std::fs::read(&trunc).unwrap();
    std::fs::write(&trunc, &bytes[..bytes.len() / 3]).unwrap();
    if let ThumbOutcome::Generated(d) = get_thumbnail(spec_for(&trunc, 64, 64)).await.unwrap() {
        assert!(image::load_from_memory(&d).is_ok(), "output must decode");
    }
}

#[tokio::test]
async fn cover_sidecar_thumbnail() {
    let dir = fresh_dir("cover");
    // song.mp3 (garbage body ok — sidecar wins before lofty parses) +
    // cover.jpg next to it.
    let song = dir.path().join("song.mp3");
    std::fs::write(&song, b"not really an mp3").unwrap();
    let cover = dir.path().join("cover.jpg");
    write_jpeg(&cover, 500, 400);
    let spec = spec_for(&song, 128, 128);
    let out = get_thumbnail(spec.clone()).await.unwrap();
    let ThumbOutcome::Generated(d) = out else {
        panic!("sidecar cover must generate");
    };
    assert!(d.starts_with(&[0xFF, 0xD8]));
    let cp = thumb_cache_path(
        &crate::media::paths::detect().cache_dir,
        &spec.path,
        spec.w,
        spec.h,
        spec.quality,
        spec.mod_unix,
        spec.file_size,
    );
    assert!(cp.exists(), "cover thumb must be cached");
    std::fs::remove_file(&cp).ok();
    lru::debug_clear();
}

#[tokio::test]
async fn progressive_photo_dc_fast_path_end_to_end() {
    // Real progressive fixture (vips/libjpeg scan script with a trailing
    // DC refinement scan) must flow through the DC-only path and produce
    // a correct 1200x800 -> 128x85-ish thumbnail.
    let p = std::path::PathBuf::from(concat!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../plain-rs"),
        "/testdata"
    ))
    .join("v-photo-prog.jpg");
    let spec = spec_for(&p, 128, 128);
    let out = get_thumbnail(spec.clone()).await.unwrap();
    let ThumbOutcome::Generated(d) = out else {
        panic!("progressive photo must generate");
    };
    let decoded = image::load_from_memory(&d).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (128, 85));
    // Cleanup.
    let cp = thumb_cache_path(
        &crate::media::paths::detect().cache_dir,
        &spec.path,
        spec.w,
        spec.h,
        spec.quality,
        spec.mod_unix,
        spec.file_size,
    );
    std::fs::remove_file(&cp).ok();
    lru::debug_clear();
}

#[tokio::test]
async fn video_thumbnail_end_to_end_and_cache_hit() {
    let mp4 = std::path::PathBuf::from(concat!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../plain-rs"),
        "/testdata"
    ))
    .join("video-h264-high.mp4");
    let spec = spec_for(&mp4, 256, 256);
    let out = get_thumbnail(spec.clone()).await.unwrap();
    let ThumbOutcome::Generated(d1) = out else {
        panic!("cover-less mp4 must generate a frame thumbnail");
    };
    assert!(d1.starts_with(&[0xFF, 0xD8]));
    assert!(image::load_from_memory(&d1).is_ok());

    let cp = thumb_cache_path(
        &crate::media::paths::detect().cache_dir,
        &spec.path,
        spec.w,
        spec.h,
        spec.quality,
        spec.mod_unix,
        spec.file_size,
    );
    assert!(cp.exists(), "video thumb must be cached");

    // Second call: served from cache, byte-identical, no regeneration.
    let key = cp.to_string_lossy().into_owned();
    let before = generations_for(&key);
    let out2 = get_thumbnail(spec.clone()).await.unwrap();
    let ThumbOutcome::Generated(d2) = out2 else {
        panic!("cached video thumb must be Generated");
    };
    assert_eq!(d1, d2);
    assert_eq!(generations_for(&key), before);

    // Non-MP4 / undecodable containers must Err (→ 204 upstream), not panic.
    let tmp = tempfile::tempdir().unwrap();
    let mkv = tmp.path().join("clip.mkv");
    std::fs::write(&mkv, b"matroska-ish garbage").unwrap();
    assert!(get_thumbnail(spec_for(&mkv, 128, 128)).await.is_err());

    std::fs::remove_file(&cp).ok();
    lru::debug_clear();
}

#[tokio::test]
async fn real_photos_end_to_end() {
    let dir = concat!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../plain-rs"),
        "/tmp-bench-media"
    );
    for (name, w, h) in [
        ("photo-4000x3000.jpg", 4000u32, 3000u32),
        ("photo-4917x3456.jpg", 4917, 3456),
        ("photo-3840x2880.png", 3840, 2880),
    ] {
        let p = std::path::PathBuf::from(dir).join(name);
        if !p.exists() {
            println!("SKIP real_photos_end_to_end: missing {}", p.display());
            continue;
        }
        for target in [256u32, 512, 1024] {
            let spec = spec_for(&p, target, target);
            let out = get_thumbnail(spec.clone()).await.unwrap();
            let ThumbOutcome::Generated(d) = out else {
                panic!("{name}@{target} must generate");
            };
            let decoded = image::load_from_memory(&d).unwrap();
            // Aspect-preserving box fit with ±1 rounding slack.
            let expect_w = target.min(w);
            let expect_h = target.min(h);
            let scale = f64::from(expect_w.min(expect_h)) / f64::from(w.min(h));
            let _ = scale;
            let (ew, eh) = if w <= target && h <= target {
                (w, h)
            } else {
                let r = f64::from(target) / f64::from(w.max(h));
                (
                    (f64::from(w) * r).round() as u32,
                    (f64::from(h) * r).round() as u32,
                )
            };
            assert!(
                (decoded.width() as i32 - ew as i32).abs() <= 1
                    && (decoded.height() as i32 - eh as i32).abs() <= 1,
                "{name}@{target}: got {}x{}, expected ~{ew}x{eh}",
                decoded.width(),
                decoded.height()
            );
            // Cleanup real cache dir entries.
            let cp = thumb_cache_path(
                &crate::media::paths::detect().cache_dir,
                &spec.path,
                spec.w,
                spec.h,
                spec.quality,
                spec.mod_unix,
                spec.file_size,
            );
            std::fs::remove_file(&cp).ok();
        }
    }
    lru::debug_clear();
}
