//! Unit tests for `src/media/metadata.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

// ---------------------------------------------------------------------------
// probe_duration_secs (container walker + lofty)
// ---------------------------------------------------------------------------

#[test]
fn probe_duration_secs_reads_mvhd_for_video_only_mp4() {
    // lofty rejects video-only MP4 ("no audio tracks"); the container walker
    // must not. Covers head-moov and tail-moov (hvc1 remux) fixtures.
    for (name, secs) in [
        ("video-h264-baseline.mp4", 6),
        ("video-h264-65s.mp4", 65),
        ("video-h264-high.mp4", 6),
        ("video-h265.mp4", 3),
        ("video-h265-hvc1.mp4", 3),
    ] {
        let path = format!("{}/testdata/{name}", env!("CARGO_MANIFEST_DIR"));
        assert_eq!(probe_duration_secs(&path), Some(secs), "{name}");
    }
}

#[test]
fn probe_duration_secs_garbage_and_missing_are_zero() {
    assert_eq!(probe_duration_secs("/nonexistent/x.mp4"), None);
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("x.mp4");
    std::fs::write(&p, b"not an mp4 at all").unwrap();
    assert_eq!(probe_duration_secs(p.to_str().unwrap()), None);
    // Truncated moov header (8 bytes claiming more) must not panic or loop.
    std::fs::write(&p, b"\x00\x00\x00\x40moov\x00\x00\x00\x10mvhd").unwrap();
    assert_eq!(probe_duration_secs(p.to_str().unwrap()), None);
}

// ---------------------------------------------------------------------------
// mvhd parsing (synthetic boxes — pure function, byte-level assertions)
// ---------------------------------------------------------------------------

/// One moov child box: header (size + kind) + payload.
fn box_bytes(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let size = (8 + payload.len()) as u32;
    [size.to_be_bytes().as_slice(), kind, payload].concat()
}

/// mvhd payload (everything past the box header) for version 0 / 1.
fn mvhd_payload_v0(timescale: u32, duration: u32) -> Vec<u8> {
    let mut b = vec![0u8; 4]; // version 0 + flags
    b.extend_from_slice(&[0u8; 8]); // creation + modification (4 + 4)
    b.extend_from_slice(&timescale.to_be_bytes());
    b.extend_from_slice(&duration.to_be_bytes());
    b
}

fn mvhd_payload_v1(timescale: u32, duration: u64) -> Vec<u8> {
    let mut b = vec![1u8, 0, 0, 0]; // version 1 + flags
    b.extend_from_slice(&[0u8; 16]); // creation + modification (8 + 8)
    b.extend_from_slice(&timescale.to_be_bytes());
    b.extend_from_slice(&duration.to_be_bytes());
    b
}

#[test]
fn mvhd_version_0_and_1_read_duration_over_timescale() {
    let moov = box_bytes(b"mvhd", &mvhd_payload_v0(10, 65));
    assert_eq!(mvhd_duration_secs(&moov), Some(6)); // 65/10 → 6 whole secs
    let moov = box_bytes(b"mvhd", &mvhd_payload_v1(1000, 6_500));
    assert_eq!(mvhd_duration_secs(&moov), Some(6));
    // Whole seconds only, truncated.
    let moov = box_bytes(b"mvhd", &mvhd_payload_v0(3, 10));
    assert_eq!(mvhd_duration_secs(&moov), Some(3));
}

#[test]
fn mvhd_found_behind_sibling_boxes() {
    let moov = [
        box_bytes(b"trak", &[0u8; 12]),
        box_bytes(b"mvhd", &mvhd_payload_v0(2, 9)),
        box_bytes(b"trak", &[0u8; 12]),
    ]
    .concat();
    assert_eq!(mvhd_duration_secs(&moov), Some(4));
}

#[test]
fn mvhd_malformed_is_none_never_panic() {
    // Payloadless mvhd (header-only box) must not index out of bounds.
    assert_eq!(mvhd_duration_secs(&box_bytes(b"mvhd", &[])), None);
    // Truncated version-0 payload (timescale present, duration missing).
    assert_eq!(mvhd_duration_secs(&box_bytes(b"mvhd", &[0u8; 16])), None);
    // Zero timescale would divide by zero.
    assert_eq!(
        mvhd_duration_secs(&box_bytes(b"mvhd", &mvhd_payload_v0(0, 10))),
        None
    );
    // No mvhd at all.
    assert_eq!(mvhd_duration_secs(&box_bytes(b"trak", &[0u8; 8])), None);
}

// ---------------------------------------------------------------------------
// probe_media (single-parse audio probe)
// ---------------------------------------------------------------------------

#[test]
fn probe_media_audio_gets_duration_and_tags() {
    let path = format!("{}/testdata/audio-tagged.mp3", env!("CARGO_MANIFEST_DIR"));
    let m = probe_media(&path, "audio");
    assert!(m.duration_secs > 0);
    assert_eq!(m.artist, "Hydrate Artist");
    assert_eq!(m.title, "Hydrate Title");
}

#[test]
fn probe_media_tagless_audio_is_duration_only() {
    let path = format!("{}/testdata/audio-untagged.mp3", env!("CARGO_MANIFEST_DIR"));
    let m = probe_media(&path, "audio");
    assert_eq!(m.duration_secs, 4);
    assert_eq!(m.artist, "");
    assert_eq!(m.title, "");
}

#[test]
fn probe_media_video_only_mp4_gets_duration_not_tags() {
    let path = format!(
        "{}/testdata/video-h264-baseline.mp4",
        env!("CARGO_MANIFEST_DIR")
    );
    let m = probe_media(&path, "video");
    assert_eq!(m.duration_secs, 6);
    assert_eq!(m.artist, "");
    assert_eq!(m.title, "");
}

#[test]
fn probe_media_missing_file_is_all_zeros() {
    let m = probe_media("/nonexistent/x.mp3", "audio");
    assert_eq!(
        (m.duration_secs, m.artist.as_str(), m.title.as_str()),
        (0, "", "")
    );
}
