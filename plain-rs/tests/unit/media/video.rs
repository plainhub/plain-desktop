//! Unit tests for `src/media/video.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

#[test]
fn filter_codec_tag_strips_non_alphanumeric() {
    assert_eq!(filter_codec_tag("hvc1"), "hvc1");
    assert_eq!(filter_codec_tag("avc1"), "avc1");
    // Untagged streams (mkv) carry `[0][0][0][0]`; the digits survive the
    // filter just like in plain-app — harmless, never hvc1/hev1.
    assert_eq!(filter_codec_tag("[0][0][0][0]"), "0000");
    assert_eq!(filter_codec_tag(""), "");
}

#[test]
fn transcode_cache_path_deterministic_and_mtime_sensitive() {
    let dir = std::path::Path::new("/tmp/cache");
    let a = transcode_cache_path(dir, "/a/v.mp4", 1000, 5000);
    let b = transcode_cache_path(dir, "/a/v.mp4", 1000, 5000);
    assert_eq!(a, b);
    assert!(a.to_str().unwrap().contains("videos/"));
    assert!(a.to_str().unwrap().ends_with(".mp4"));
    let c = transcode_cache_path(dir, "/a/v.mp4", 1001, 5000);
    assert_ne!(a, c);
}

// ---- in-process ISO-BMFF probe: synthetic box builders ----

fn box_of(kind: &str, payload: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(8 + payload.len());
    b.extend_from_slice(&(8u32 + payload.len() as u32).to_be_bytes());
    b.extend_from_slice(kind.as_bytes());
    b.extend_from_slice(payload);
    b
}

/// mdat with the 64-bit largesize form (size == 1).
fn mdat_largesize(payload: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(16 + payload.len());
    b.extend_from_slice(&1u32.to_be_bytes());
    b.extend_from_slice(b"mdat");
    b.extend_from_slice(&(16u64 + payload.len() as u64).to_be_bytes());
    b.extend_from_slice(payload);
    b
}

fn trak_with(handler: &[u8; 4], entry_fourcc: &str) -> Vec<u8> {
    let mut stsd_payload = Vec::new();
    stsd_payload.extend_from_slice(&[0u8; 8]); // version/flags + entry_count
    stsd_payload.extend_from_slice(&78u32.to_be_bytes()); // entry size
    stsd_payload.extend_from_slice(entry_fourcc.as_bytes());
    stsd_payload.extend_from_slice(&[0u8; 8]); // sample entry body stub
    let stbl = box_of("stbl", &box_of("stsd", &stsd_payload));
    let mut hdlr = vec![0u8; 24]; // version/flags + pre_defined + handler + reserved[3]
    hdlr[8..12].copy_from_slice(handler);
    let mdia = box_of(
        "mdia",
        &[box_of("hdlr", &hdlr), box_of("minf", &stbl)].concat(),
    );
    box_of("trak", &mdia)
}

/// Write `flat` to a temp file; keep the returned TempDir alive for as
/// long as the path is used (it removes the file when dropped).
fn write_temp(flat: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("probe.mp4");
    std::fs::write(&path, flat).unwrap();
    (dir, path)
}

#[test]
fn probe_walks_synthetic_moov() {
    let (_d, file) = write_temp(
        &[
            box_of("ftyp", b"isom"),
            box_of("moov", &trak_with(b"vide", "hvc1")),
        ]
        .concat(),
    );
    assert_eq!(probe_iso_bmff_video_fourcc(&file).as_deref(), Some("hvc1"));
}

#[test]
fn probe_picks_video_trak_after_audio_trak() {
    // ffprobe's `v:0` picks the first *video* stream regardless of trak
    // order — audio-first files must not be misread.
    let (_d, file) = write_temp(
        &[box_of(
            "moov",
            &[trak_with(b"soun", "mp4a"), trak_with(b"vide", "avc1")].concat(),
        )]
        .concat(),
    );
    assert_eq!(probe_iso_bmff_video_fourcc(&file).as_deref(), Some("avc1"));
}

#[test]
fn probe_audio_only_and_largesize_mdat_at_end() {
    let (_d, audio_only) = write_temp(&[box_of("moov", &trak_with(b"soun", "mp4a"))].concat());
    assert_eq!(probe_iso_bmff_video_fourcc(&audio_only), None);

    // moov after an mdat that uses the 64-bit largesize form: the walker
    // must skip it by declared size and still find the trailing moov.
    let (_d, moov_at_end) = write_temp(
        &[
            mdat_largesize(&[0xAA; 64]),
            box_of("moov", &trak_with(b"vide", "hev1")),
        ]
        .concat(),
    );
    assert_eq!(
        probe_iso_bmff_video_fourcc(&moov_at_end).as_deref(),
        Some("hev1")
    );
}

#[test]
fn probe_garbage_and_truncation_is_none() {
    let (_d, garbage) = write_temp(&[vec![0x13; 300]].concat());
    assert_eq!(probe_iso_bmff_video_fourcc(&garbage), None);

    let (_d, empty) = write_temp(b"");
    assert_eq!(probe_iso_bmff_video_fourcc(&empty), None);

    // Header claiming a moov larger than the file: truncated read → None
    // (never a panic, never a partial parse).
    let mut lying = Vec::new();
    lying.extend_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
    lying.extend_from_slice(b"moov");
    let (_d, lying_path) = write_temp(&lying);
    assert_eq!(probe_iso_bmff_video_fourcc(&lying_path), None);
}

// ---- in-process probe: real fixtures (locks the fast path, not ffprobe) ----

fn testdata(name: &str) -> String {
    format!("{}/testdata/{}", env!("CARGO_MANIFEST_DIR"), name)
}

#[test]
fn probe_reads_fourcc_from_real_fixtures() {
    let p = std::path::Path::new;
    assert_eq!(
        probe_iso_bmff_video_fourcc(p(&testdata("video-h264-high.mp4"))).as_deref(),
        Some("avc1")
    );
    assert_eq!(
        probe_iso_bmff_video_fourcc(p(&testdata("video-h265.mp4"))).as_deref(),
        Some("hev1")
    );
    // hvc1 is the case HEVC-less browsers care about most, and this
    // fixture carries its moov at the end of the file.
    assert_eq!(
        probe_iso_bmff_video_fourcc(p(&testdata("video-h265-hvc1.mp4"))).as_deref(),
        Some("hvc1")
    );
}

#[tokio::test]
async fn iso_bmff_probe_never_spawns_ffprobe() {
    // Performance contract: MP4/MOV probes must answer from the
    // in-process walker. A spawn here costs ~0.5 s per first view on a
    // weak NAS CPU (ffprobe parsing a 4K file pegs a core) — this is
    // the regression the walker exists to prevent.
    use std::sync::atomic::{AtomicBool, Ordering};
    let called = std::sync::Arc::new(AtomicBool::new(false));
    let flag = called.clone();
    let codec = probe_with_fallback(&testdata("video-h264-high.mp4"), move |_: String| {
        let flag = flag.clone();
        async move {
            flag.store(true, Ordering::SeqCst);
            "FFPROBE".to_string()
        }
    })
    .await;
    assert_eq!(codec, "avc1");
    assert!(
        !called.load(Ordering::SeqCst),
        "ISO-BMFF probe must not fall back to the ffprobe process"
    );

    // The hvc1 flavor too — the case HEVC-less browsers care about.
    let codec = probe_with_fallback(&testdata("video-h265-hvc1.mp4"), |_: String| async {
        "FFPROBE".to_string()
    })
    .await;
    assert_eq!(codec, "hvc1");
}

#[tokio::test]
async fn non_iso_container_falls_back_to_ffprobe() {
    // Garbage bytes: the walker declines, the injected fallback answers.
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("video.mkv");
    std::fs::write(&p, b"definitely not an iso-bmff container").unwrap();
    let codec = probe_with_fallback(&p.to_string_lossy(), |_: String| async {
        "FFPROBE".to_string()
    })
    .await;
    assert_eq!(codec, "FFPROBE");
}

#[tokio::test]
async fn probe_video_codec_end_to_end_avoids_ffprobe_latency() {
    // End-to-end: the public handler-facing probe must answer from the
    // in-process walker (sub-millisecond) — asserting the exact fourcc
    // keeps a regression to the ffprobe spawn visible as a latency cliff.
    let codec = probe_video_codec(&testdata("video-h265-hvc1.mp4")).await;
    assert_eq!(codec, "hvc1");
}
