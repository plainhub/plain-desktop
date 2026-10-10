//! Unit tests for `src/media/thumb_engine/video.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

fn testdata(name: &str) -> String {
    format!(
        "{}/testdata/{}",
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../plain-rs"),
        name
    )
}

#[test]
fn compute_video_timepoint_matrix() {
    assert_eq!(compute_video_timepoint(2), 0.0);
    assert_eq!(compute_video_timepoint(30), 1.0);
    assert_eq!(compute_video_timepoint(120), 12.0);
    assert_eq!(compute_video_timepoint(0), 0.0);
}

#[test]
fn avcc_to_annexb_converts_and_validates() {
    // Two NALs, 4-byte lengths: {01 02 03} and {aa}.
    let mut out = Vec::new();
    length_prefixed_to_annexb(&[0, 0, 0, 3, 1, 2, 3, 0, 0, 0, 1, 0xaa], 4, &mut out).unwrap();
    assert_eq!(out, vec![0, 0, 0, 1, 1, 2, 3, 0, 0, 0, 1, 0xaa]);
    // 2-byte length prefixes.
    let mut out = Vec::new();
    length_prefixed_to_annexb(&[0, 2, 0xbb, 0xcc, 0, 1, 0xdd], 2, &mut out).unwrap();
    assert_eq!(out, vec![0, 0, 0, 1, 0xbb, 0xcc, 0, 0, 0, 1, 0xdd]);
    // 3-byte prefix (nal_length_size_minus_one = 2).
    let mut out = Vec::new();
    length_prefixed_to_annexb(&[0, 0, 1, 0xee], 3, &mut out).unwrap();
    assert_eq!(out, vec![0, 0, 0, 1, 0xee]);

    // Errors: declared length overruns the buffer.
    assert!(length_prefixed_to_annexb(&[0, 0, 0, 9, 1, 2], 4, &mut Vec::new()).is_err());
    // Zero-length NAL.
    assert!(length_prefixed_to_annexb(&[0, 0, 0, 0, 1], 4, &mut Vec::new()).is_err());
    // Trailing bytes shorter than the length prefix size.
    assert!(length_prefixed_to_annexb(&[0, 0, 0, 1, 9, 7], 4, &mut Vec::new()).is_err());
    // Absurd length size.
    assert!(length_prefixed_to_annexb(&[1], 5, &mut Vec::new()).is_err());
}

/// Hand-built sample tables: 12 samples, stts runs + stss syncs.
#[test]
fn keyframe_location_matrix() {
    // Uniform 1000-tick deltas, syncs at {1, 5, 9}.
    let stts = [(6u64, 1000u64), (6, 1000)];
    let syncs = [1u32, 5, 9];
    // t=0 → sample 1 (sync).
    assert_eq!(locate_keyframe_sample(&stts, Some(&syncs), 0), 1);
    // t=1.5s → time sample 2, previous sync = 1.
    assert_eq!(locate_keyframe_sample(&stts, Some(&syncs), 1_500), 1);
    // t=4.9s → time sample 5 (itself sync) → 5.
    assert_eq!(locate_keyframe_sample(&stts, Some(&syncs), 4_900), 5);
    // t=7s → time sample 8, previous sync = 5.
    assert_eq!(locate_keyframe_sample(&stts, Some(&syncs), 7_000), 5);
    // t=9.999s → sample 10, sync 9.
    assert_eq!(locate_keyframe_sample(&stts, Some(&syncs), 9_999), 9);
    // Beyond the end → walk clamps past the last sample → sync 9.
    assert_eq!(locate_keyframe_sample(&stts, Some(&syncs), 999_999), 9);

    // No stss → every sample is sync: plain timestamp walk.
    assert_eq!(locate_keyframe_sample(&stts, None, 7_000), 8);
    assert_eq!(locate_keyframe_sample(&stts, None, 0), 1);

    // Variable-delta runs: 2 samples @500 then 10 @1250.
    let var = [(2u64, 500u64), (10, 1250)];
    // t=600 → inside the first run → sample 2.
    assert_eq!(locate_keyframe_sample(&var, None, 600), 2);
    // t=1300: first run covers ticks [0,1000) samples 1-2; second run
    // starts at sample 3 tick 1000, delta 1250 → (1300-1000)/1250=0 → 3.
    assert_eq!(locate_keyframe_sample(&var, None, 1_300), 3);
    // t=10000 → (10000-1000)/1250 = 7 → sample 10.
    assert_eq!(locate_keyframe_sample(&var, None, 10_000), 10);

    // Syncs not starting at 1 (open-GOP-ish): target before first sync.
    let late_syncs = [5u32, 9];
    assert_eq!(locate_keyframe_sample(&stts, Some(&late_syncs), 900), 5);
}

#[test]
fn yuv_to_rgb_known_colors() {
    // Limited-range BT.601 anchors (see yuv420_to_rgb). 4×2 frame: left
    // half red, right half white — luma per pixel, chroma per 2×2 block.
    let mut f = YuvFrame {
        width: 4,
        height: 2,
        y: vec![81, 81, 235, 235, 81, 81, 235, 235],
        u: vec![90, 128],
        v: vec![240, 128],
    };
    f.y.shrink_to_fit();
    let bm = yuv420_to_rgb(&f);
    assert_eq!((bm.width(), bm.height(), bm.channels()), (4, 2, 3));
    let (d, _, _) = bm.raw();
    let px = |i: usize| (d[i * 3], d[i * 3 + 1], d[i * 3 + 2]);
    let red_px = px(0);
    assert!(
        red_px.0 > 240 && red_px.1 < 16 && red_px.2 < 16,
        "anchor red, got {red_px:?}"
    );
    assert!(px(4).0 > 240 && px(4).2 < 16, "row 2 left is red too");
    let white_px = px(2);
    assert!(
        white_px.0 > 230 && white_px.1 > 230 && white_px.2 > 230,
        "anchor white, got {white_px:?}"
    );
    // Black anchor in a separate 2×2 frame.
    let black = YuvFrame {
        width: 2,
        height: 2,
        y: vec![16, 16, 16, 16],
        u: vec![128],
        v: vec![128],
    };
    let bm = yuv420_to_rgb(&black);
    let (d, _, _) = bm.raw();
    assert!(d.iter().all(|&v| v < 16), "anchor black, got {:?}", &d[..6]);
}

#[test]
fn error_paths_do_not_panic() {
    // HEVC track: gated at the avc1 check with a clear message.
    let err = plan_keyframe(&testdata("video-h265.mp4")).unwrap_err();
    assert!(
        err.to_string().contains("no pure-Rust decoder"),
        "got: {err}"
    );
    // Garbage bytes with an mp4 extension.
    let tmp = tempfile::tempdir().unwrap();
    let garbage = tmp.path().join("garbage.mp4");
    std::fs::write(&garbage, b"definitely not an mp4 file at all....").unwrap();
    assert!(plan_keyframe(garbage.to_str().unwrap()).is_err());
    // Truncated real MP4: moov survives, sample read fails or NAL parse
    // fails — either way an Err, never a panic.
    let full = std::fs::read(testdata("video-h264-baseline.mp4")).unwrap();
    let trunc = tmp.path().join("trunc.mp4");
    std::fs::write(&trunc, &full[..full.len() / 3]).unwrap();
    let _ = plan_keyframe(trunc.to_str().unwrap());
    // Empty file.
    let empty = tmp.path().join("empty.mp4");
    std::fs::write(&empty, b"").unwrap();
    assert!(plan_keyframe(empty.to_str().unwrap()).is_err());
}

#[test]
fn end_to_end_h264_mp4_frame() {
    for name in ["video-h264-baseline.mp4", "video-h264-high.mp4"] {
        let path = testdata(name);
        let jpg = generate_video_thumbnail(&path, 256, 256, 75).unwrap();
        assert!(jpg.starts_with(&[0xFF, 0xD8]), "{name}: JPEG magic");
        let decoded = image::load_from_memory(&jpg).unwrap();
        assert_eq!(
            (decoded.width(), decoded.height()),
            (256, 192),
            "{name}: 320x240 → 256 box"
        );
        // The frame must carry real content (testsrc2 gradients), not a
        // blank/garbage decode.
        let img = decoded.to_rgb8();
        let n = f64::from(img.width() * img.height());
        let mut mean = 0f64;
        for p in img.pixels() {
            mean += (f64::from(p[0]) + f64::from(p[1]) + f64::from(p[2])) / 3.0;
        }
        mean /= n;
        let mut var = 0f64;
        for p in img.pixels() {
            let m = (f64::from(p[0]) + f64::from(p[1]) + f64::from(p[2])) / 3.0;
            var += (m - mean) * (m - mean);
        }
        let std_dev = (var / n).sqrt();
        assert!(
            std_dev > 20.0,
            "{name}: decoded frame looks blank (stddev {std_dev:.1})"
        );
    }
}

#[test]
fn long_video_targets_ten_percent_timepoint() {
    // 65 s video → timepoint 6.5 s, well past the first GOP: the plan
    // must start at a mid-stream keyframe, not sample 1.
    let plan = plan_keyframe(&testdata("video-h264-65s.mp4")).unwrap();
    assert!(plan.first_sample_id > 1, "expected a mid-stream keyframe");
    assert!(plan.estimated_bytes() > 0);
    let jpg = decode_plan_to_jpeg(plan, 256, 256, 75).unwrap();
    assert!(image::load_from_memory(&jpg).is_ok());
}
