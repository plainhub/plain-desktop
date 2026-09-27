//! Unit tests for `src/media/thumb_engine/exif.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

fn jpeg_with_orientation(v: u8) -> Vec<u8> {
    let mut v2 = vec![0xFF, 0xD8];
    v2.extend_from_slice(&test_app1_orientation(v));
    // Trailing SOI-ish bytes so the marker walk has room.
    v2.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08, 0x03, 0xE8, 0x02, 0x80, 0x01]);
    v2
}

#[test]
fn orientation_all_values() {
    for v in 1u8..=8 {
        assert_eq!(orientation(&jpeg_with_orientation(v)), Some(v), "v={v}");
    }
    assert!(Orientation(6).swaps_axes(), "orientation 6 swaps axes");
    assert!(!Orientation(3).swaps_axes());
}

#[test]
fn orientation_big_endian_tiff() {
    let mut tiff: Vec<u8> = b"MM".to_vec();
    tiff.extend_from_slice(&42u16.to_be_bytes());
    tiff.extend_from_slice(&8u32.to_be_bytes());
    tiff.extend_from_slice(&1u16.to_be_bytes());
    tiff.extend_from_slice(&0x0112u16.to_be_bytes());
    tiff.extend_from_slice(&3u16.to_be_bytes());
    tiff.extend_from_slice(&1u32.to_be_bytes());
    tiff.extend_from_slice(&8u16.to_be_bytes());
    tiff.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    let mut jpg = vec![0xFF, 0xD8, 0xFF, 0xE1];
    jpg.push(((8 + tiff.len() + 2) >> 8) as u8);
    jpg.push(((8 + tiff.len() + 2) & 0xFF) as u8);
    jpg.extend_from_slice(b"Exif\0\0");
    jpg.extend_from_slice(&tiff);
    assert_eq!(orientation(&jpg), Some(8));
}

#[test]
fn orientation_absent_cases() {
    // No APP1 at all (hits SOS).
    assert_eq!(orientation(&[0xFF, 0xD8, 0xFF, 0xDA, 0x00]), None);
    // APP1 with XMP payload (not Exif).
    let mut jpg = vec![0xFF, 0xD8, 0xFF, 0xE1, 0x00, 0x0A];
    jpg.extend_from_slice(b"http://wp");
    assert_eq!(orientation(&jpg), None);
    // IFD without the tag.
    let mut tiff = b"II".to_vec();
    tiff.extend_from_slice(&42u16.to_le_bytes());
    tiff.extend_from_slice(&8u32.to_le_bytes());
    tiff.extend_from_slice(&0u16.to_le_bytes()); // 0 entries
    let mut jpg = vec![0xFF, 0xD8, 0xFF, 0xE1];
    let n = (6 + tiff.len() + 2) as u16;
    jpg.extend_from_slice(&n.to_be_bytes());
    jpg.extend_from_slice(b"Exif\0\0");
    jpg.extend_from_slice(&tiff);
    assert_eq!(orientation(&jpg), None);
    // Truncated buffer.
    assert_eq!(orientation(&[0xFF, 0xD8, 0xFF]), None);
}

#[test]
fn orientation_real_bench_photos() {
    // The two Commons photos carry EXIF (Canon cameras); orientation is
    // commonly 1. Assert only that parsing does not misfire, and that a
    // valid value comes back when the segment exists.
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tmp-bench-media");
    for name in ["photo-4000x3000.jpg", "photo-4917x3456.jpg"] {
        let p = format!("{dir}/{name}");
        let Ok(head) = std::fs::read(&p) else {
            println!("SKIP orientation_real_bench_photos: missing {p}");
            continue;
        };
        if let Some(v) = orientation(&head) {
            assert!((1..=8).contains(&v), "{name}: bogus orientation {v}");
            println!("{name}: orientation = {v}");
        } else {
            println!("{name}: no EXIF orientation");
        }
    }
}
