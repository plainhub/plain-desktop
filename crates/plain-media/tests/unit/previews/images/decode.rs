//! Unit tests for `src/media/thumb_engine/decode.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

fn write_test_jpeg(path: &std::path::Path, w: u32, h: u32) {
    let img = image::RgbImage::from_fn(w, h, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
    });
    img.save_with_format(path, image::ImageFormat::Jpeg)
        .unwrap();
}

#[test]
fn predict_scaled_matches_decoder_on_real_images() {
    let dir = concat!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../plain-rs"),
        "/tmp-bench-media"
    );
    for (name, w, h) in [
        ("photo-4000x3000.jpg", 4000u32, 3000u32),
        ("photo-4917x3456.jpg", 4917, 3456),
    ] {
        let p = std::path::PathBuf::from(dir).join(name);
        if !p.exists() {
            println!("SKIP predict_scaled: missing {}", p.display());
            continue;
        }
        for (tw, th) in [(256u32, 256u32), (512, 512), (1024, 1024)] {
            let file = std::fs::File::open(&p).unwrap();
            let mut dec = jpeg_decoder::Decoder::new(std::io::BufReader::new(file));
            dec.read_info().unwrap();
            dec.scale(tw as u16, th as u16).unwrap();
            dec.decode().unwrap();
            let info = dec.info().unwrap();
            let actual = (u32::from(info.width), u32::from(info.height));
            let predicted = predict_scaled(w, h, tw, th);
            assert_eq!(actual, predicted, "{name} -> {tw}x{th}");
        }
    }
}

#[test]
fn jpeg_scaled_decodes_smaller_and_correct() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("src.jpg");
    write_test_jpeg(&p, 1024, 768);
    let bm = jpeg_scaled(&p, 256, 256).unwrap();
    // 1024/4 = 256 ≥ 256 → scale 1/4 expected.
    assert_eq!((bm.width(), bm.height()), (256, 192));
    assert_eq!(bm.channels(), 3);
    // The scaled decode must remain a faithful image (not blank/corrupt):
    // decode reference via full decode + box average of corner block.
    let full = image_full(&p).unwrap();
    let (d, _, _) = full.raw();
    // Corner block average (first 4x4) of the source:
    let mut mean = [0f64; 3];
    for y in 0..4 {
        for x in 0..4 {
            for c in 0..3 {
                mean[c] += d[(y * 1024 + x) * 3 + c] as f64;
            }
        }
    }
    for m in mean.iter_mut() {
        *m /= 16.0;
    }
    let (sd, _, _) = bm.raw();
    // Top-left pixel of scaled image should be near the source corner mean
    // (gradient image: x≈0..3 → ~1, y≈0..3 → ~1).
    assert!(
        (sd[0] as f64 - mean[0]).abs() < 4.0,
        "px0 {} vs mean {}",
        sd[0],
        mean[0]
    );
}

#[test]
fn jpeg_scaled_grayscale() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("gray.jpg");
    let img = image::GrayImage::from_fn(512, 512, |x, _| image::Luma([(x % 256) as u8]));
    img.save_with_format(&p, image::ImageFormat::Jpeg).unwrap();
    let bm = jpeg_scaled(&p, 128, 128).unwrap();
    assert_eq!(bm.channels(), 3);
    assert_eq!(bm.width(), 128);
}

#[test]
fn jpeg_scaled_upscale_request_falls_back_to_full() {
    // Request bigger than source: scale() must not blow up.
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("tiny.jpg");
    write_test_jpeg(&p, 64, 48);
    let bm = jpeg_scaled(&p, 512, 512).unwrap();
    assert_eq!((bm.width(), bm.height()), (64, 48));
}

#[test]
fn image_full_png_webp() {
    let tmp = tempfile::tempdir().unwrap();
    let png = tmp.path().join("t.png");
    image::RgbaImage::from_fn(100, 80, |x, y| image::Rgba([x as u8, y as u8, 55, 255]))
        .save_with_format(&png, image::ImageFormat::Png)
        .unwrap();
    let bm = image_full(&png).unwrap();
    assert_eq!((bm.width(), bm.height(), bm.channels()), (100, 80, 4));

    // WebP round-trip via the image crate encoder (lossless).
    let webp = tmp.path().join("t.webp");
    let img = image::RgbImage::from_fn(64, 32, |x, _| image::Rgb([x as u8, 3, 4]));
    img.save_with_format(&webp, image::ImageFormat::WebP)
        .unwrap();
    let bm = image_full(&webp).unwrap();
    assert_eq!((bm.width(), bm.height(), bm.channels()), (64, 32, 3));
}

#[test]
fn image_from_bytes_and_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("x.jpg");
    write_test_jpeg(&p, 32, 32);
    let bytes = std::fs::read(&p).unwrap();
    let bm = image_from_bytes(&bytes).unwrap();
    assert_eq!(bm.width(), 32);

    // Errors: empty, garbage, truncated JPEG header.
    assert!(image_from_bytes(&[]).is_err());
    assert!(image_from_bytes(b"garbage bytes!").is_err());
    let mut trunc = bytes[..20].to_vec();
    trunc[0] = 0xFF;
    assert!(image_from_bytes(&trunc).is_err());
    // Corrupt-but-sniffable file on disk (valid header, dead payload).
    let mut bad = bytes.clone();
    let cut = bad.len() / 2;
    for b in bad.iter_mut().skip(cut) {
        *b = 0x55;
    }
    assert!(image_from_bytes(&bad).is_err() || bad.len() == cut);
    assert!(image_from_bytes(&bad).is_err());
}

#[test]
fn corrupt_file_on_disk_errors_cleanly() {
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("corrupt.jpg");
    write_test_jpeg(&p, 128, 128);
    let mut bytes = std::fs::read(&p).unwrap();
    let n = bytes.len();
    for b in bytes.iter_mut().take(n / 2).skip(n / 4) {
        *b ^= 0xAA;
    }
    std::fs::write(&p, bytes).unwrap();
    // Either decoder may or may not tolerate mid-stream corruption, but
    // the call must not panic and must return a Bitmap or an error.
    let _ = jpeg_scaled(&p, 64, 64);
    let _ = image_full(&p);
}
