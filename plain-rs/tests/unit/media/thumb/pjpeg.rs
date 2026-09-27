//! Unit tests for `src/media/thumb_engine/pjpeg.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

fn fixture(name: &str) -> Vec<u8> {
    let p = std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/testdata")).join(name);
    std::fs::read(&p).unwrap_or_else(|_| panic!("missing fixture {}", p.display()))
}

/// Reference: jpeg-decoder decoding the same bytes at 1/8 scale.
fn reference_1_8(data: &[u8]) -> Bitmap {
    let mut dec = jpeg_decoder::Decoder::new(std::io::Cursor::new(data));
    dec.read_info().unwrap();
    let info = dec.info().unwrap();
    let w = u32::from(info.width);
    let h = u32::from(info.height);
    dec.scale((w / 8).max(1) as u16, (h / 8).max(1) as u16)
        .unwrap();
    let pixels = dec.decode().expect("reference decode");
    let info = dec.info().unwrap();
    if info.pixel_format == jpeg_decoder::PixelFormat::L8 {
        let mut rgb = Vec::with_capacity(pixels.len() * 3);
        for v in &pixels {
            rgb.extend_from_slice(&[*v, *v, *v]);
        }
        return Bitmap::new_rgb(rgb, u32::from(info.width), u32::from(info.height));
    }
    Bitmap::new_rgb(pixels, u32::from(info.width), u32::from(info.height))
}

fn assert_same(a: &Bitmap, b: &Bitmap, ctx: &str) {
    assert_eq!(
        (a.width(), a.height()),
        (b.width(), b.height()),
        "{ctx} dims"
    );
    let (da, _, _) = a.raw();
    let (db, _, _) = b.raw();
    // The kernels mirror jpeg-decoder's integer math exactly; measured
    // byte-identical on all fixtures. Allow nothing.
    assert!(
        da == db,
        "{ctx}: {} of {} bytes differ",
        da.iter().zip(db).filter(|(x, y)| x != y).count(),
        da.len()
    );
}

#[test]
fn parity_420_fixture() {
    let data = fixture("v-prog420.jpg");
    let mine = decode_dc_only_bytes(&data).unwrap();
    let r = reference_1_8(&data);
    assert_same(&mine, &r, "420 fixture");
    // 160x120 -> 20x15.
    assert_eq!((mine.width(), mine.height()), (20, 15));
}

#[test]
fn parity_grayscale_fixture() {
    let data = fixture("v-prog-gray.jpg");
    let mine = decode_dc_only_bytes(&data).unwrap();
    let r = reference_1_8(&data);
    assert_same(&mine, &r, "gray fixture");
    assert_eq!((mine.width(), mine.height()), (20, 15));
}

#[test]
fn parity_real_photo_fixture_and_skip_stats() {
    let data = fixture("v-photo-prog.jpg");
    let mine = decode_dc_only_bytes(&data).unwrap();
    assert_eq!((mine.width(), mine.height()), (150, 100));
    let r = reference_1_8(&data);
    assert_same(&mine, &r, "photo fixture");

    // The AC scans must be accounted as skipped: most of the entropy
    // payload never gets parsed.
    let (dc_bytes, skipped_bytes) = LAST_SKIP.lock().unwrap().unwrap_or((0, 0));
    assert!(
        skipped_bytes > dc_bytes * 2,
        "expected AC skip to dominate: dc={dc_bytes} skipped={skipped_bytes}"
    );
    println!("photo fixture: dc {dc_bytes} B decoded, {skipped_bytes} B AC skipped");
}

#[test]
fn dc_refinement_scan_is_decoded() {
    // The vips fixture's scan script has a trailing DC refinement scan
    // (Ah=1) after AC scans — it must not be skipped.
    let data = fixture("v-prog420.jpg");
    let mine = decode_dc_only_bytes(&data).unwrap();
    let r = reference_1_8(&data);
    assert_same(&mine, &r, "refinement scan parity");
}

#[test]
fn rejects_baseline_sof0() {
    // A baseline JPEG must not decode through the progressive path.
    let img = image::RgbImage::from_fn(64, 48, |x, y| image::Rgb([x as u8, y as u8, 5]));
    let mut v = Vec::new();
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut v, 80);
    enc.encode(img.as_raw(), 64, 48, image::ExtendedColorType::Rgb8)
        .unwrap();
    assert!(decode_dc_only_bytes(&v).is_err());
}

#[test]
fn rejects_malformed_streams() {
    let data = fixture("v-prog420.jpg");
    // Truncated inside the DC scan: cut early.
    let cut = &data[..data.len() / 3];
    // May or may not error depending on where the cut lands relative to
    // the DC scan, but must never panic and must produce a bitmap or Err.
    let _ = decode_dc_only_bytes(cut);

    // Garbage payload after valid headers of the gray fixture.
    let mut bad = fixture("v-prog-gray.jpg");
    let half = bad.len() / 2;
    for b in bad.iter_mut().skip(half) {
        *b = 0x5A;
    }
    let _ = decode_dc_only_bytes(&bad);

    // Not a JPEG at all.
    assert!(decode_dc_only_bytes(b"hello world, definitely not jpeg").is_err());
}

#[test]
fn h2v1_and_h2v2_row_math() {
    // Pin the fir kernels to jpeg-decoder's exact integer math.
    // H2V1: input [100, 200], out = [100, (300+200+2)/4=125? no: (3*100+200+2)>>2=125]
    let mut out = [0u8; 4];
    h2v1_row(&[100, 200], 2, 1, 0, &mut out);
    assert_eq!(
        out,
        [
            100,
            ((3 * 100 + 200 + 2) >> 2) as u8,
            ((3 * 200 + 100 + 2) >> 2) as u8,
            200
        ]
    );

    // H2V2 single column: value = (3*near + far + 2) >> 2.
    let plane = [10, 90]; // two rows, one column
    let mut o = [0u8; 2];
    h2v2_row(&plane, 1, 2, 0, &mut o); // even row: near=0, far=0 (clamped)
    assert_eq!(o, [((3 * 10 + 10 + 2) >> 2) as u8; 2]);
    h2v2_row(&plane, 1, 2, 1, &mut o); // odd row: near=0, far=1
    assert_eq!(o, [((3 * 10 + 90 + 2) >> 2) as u8; 2]);
    h2v2_row(&plane, 1, 2, 2, &mut o); // near=1, far=0
    assert_eq!(o, [((3 * 90 + 10 + 2) >> 2) as u8; 2]);
    h2v2_row(&plane, 1, 2, 3, &mut o); // near=1, far=1 (clamped)
    assert_eq!(o, [((3 * 90 + 90 + 2) >> 2) as u8; 2]);
}

#[test]
fn ycbcr_matches_jpeg_decoder_constants() {
    // Spot-check against the float reference: y=128, cb=128, cr=128 is neutral.
    assert_eq!(ycbcr_to_rgb(128, 128, 128), (128, 128, 128));
    let (r, g, b) = ycbcr_to_rgb(200, 100, 180);
    assert!(r > 200 && g < 200 && b > 100, "r={r} g={g} b={b}");
}

#[test]
fn extend_categories() {
    assert_eq!(extend(0b0, 1), -1);
    assert_eq!(extend(0b1, 1), 1);
    assert_eq!(extend(0b00, 2), -3);
    assert_eq!(extend(0b11, 2), 3);
}
