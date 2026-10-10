//! Unit tests for `src/media/thumb_engine/encode.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

fn gradient_rgb(w: u32, h: u32) -> Bitmap {
    let mut data = Vec::with_capacity(w as usize * h as usize * 3);
    for y in 0..h {
        for x in 0..w {
            data.extend_from_slice(&[x as u8, y as u8, 128]);
        }
    }
    Bitmap::new_rgb(data, w, h)
}

#[test]
fn encode_produces_valid_jpeg_header() {
    let out = encode_jpeg(gradient_rgb(64, 48), 75).unwrap();
    assert!(out.starts_with(&[0xFF, 0xD8]), "SOI marker");
    assert!(out.ends_with(&[0xFF, 0xD9]), "EOI marker");
    assert!(out.len() > 500);
    // Round-trip through the image crate to prove decodability + dims.
    let img = image::load_from_memory(&out).unwrap();
    assert_eq!((img.width(), img.height()), (64, 48));
}

#[test]
fn encode_rgba_flattens_alpha() {
    let mut data = Vec::new();
    for _ in 0..16 {
        data.extend_from_slice(&[255, 0, 0, 0]); // fully transparent red
    }
    let bm = Bitmap::new_rgba(data, 4, 4);
    let out = encode_jpeg(bm, 90).unwrap();
    let img = image::load_from_memory(&out).unwrap().to_rgba8();
    // Transparent pixels become white.
    assert_eq!(img.get_pixel(0, 0).0[..3], [255, 255, 255]);
}

#[test]
fn encode_quality_affects_size_and_decodes() {
    let bm = gradient_rgb(256, 256);
    let small = encode_jpeg(bm.clone(), 20).unwrap();
    let big = encode_jpeg(bm, 95).unwrap();
    assert!(
        small.len() < big.len(),
        "q20 {} vs q95 {}",
        small.len(),
        big.len()
    );
    assert!(image::load_from_memory(&small).is_ok());
    assert!(image::load_from_memory(&big).is_ok());
}

#[test]
fn encode_rejects_dimension_overflow() {
    // u16::MAX + 1 wide must be rejected, not silently truncated.
    let bm = Bitmap::new_rgb(vec![0u8; 65_536 * 3], 65_536, 1);
    assert!(encode_jpeg(bm, 75).is_err());
}
