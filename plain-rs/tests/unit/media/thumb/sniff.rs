//! Unit tests for `src/media/thumb_engine/sniff.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

fn minimal_jpeg(w: u16, h: u16, progressive: bool) -> Vec<u8> {
    let mut v = vec![0xFF, 0xD8];
    // APP0/JFIF segment to exercise the skip path.
    v.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x04, 0x4A, 0x46]);
    let sof: u8 = if progressive { 0xC2 } else { 0xC0 };
    v.push(0xFF);
    v.push(sof);
    v.extend_from_slice(&[0x00, 0x11, 0x08]); // len, precision
    v.extend_from_slice(&h.to_be_bytes());
    v.extend_from_slice(&w.to_be_bytes());
    v.push(0x01); // 1 component
    v
}

fn minimal_png(w: u32, h: u32) -> Vec<u8> {
    let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    v.extend_from_slice(&13u32.to_be_bytes());
    v.extend_from_slice(b"IHDR");
    v.extend_from_slice(&w.to_be_bytes());
    v.extend_from_slice(&h.to_be_bytes());
    v.extend_from_slice(&[8, 6, 0, 0, 0]);
    v
}

#[test]
fn sniff_jpeg_baseline_and_progressive() {
    let s = sniff_header(&minimal_jpeg(4000, 3000, false)).unwrap();
    assert_eq!(
        s,
        Sniffed {
            kind: ImageKind::Jpeg,
            width: 4000,
            height: 3000,
            progressive: false
        }
    );
    let s = sniff_header(&minimal_jpeg(640, 480, true)).unwrap();
    assert!(s.progressive);
}

#[test]
fn sniff_jpeg_fills_and_dqt() {
    // Fill bytes + a DQT segment before SOF must be walked correctly.
    let mut v = vec![0xFF, 0xD8, 0xFF, 0xFF, 0xFF];
    v.push(0xDB); // DQT
    v.extend_from_slice(&[0x00, 0x45]); // len 69
    v.extend_from_slice(&[0u8; 67]);
    v.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08]);
    v.extend_from_slice(&3000u16.to_be_bytes());
    v.extend_from_slice(&4000u16.to_be_bytes());
    v.push(0x03);
    let s = sniff_header(&v).unwrap();
    assert_eq!((s.width, s.height), (4000, 3000));
}

#[test]
fn sniff_png_gif_bmp_webp_vp8x() {
    let s = sniff_header(&minimal_png(3840, 2880)).unwrap();
    assert_eq!((s.kind, s.width, s.height), (ImageKind::Png, 3840, 2880));

    let mut gif = b"GIF89a".to_vec();
    gif.extend_from_slice(&320u16.to_le_bytes());
    gif.extend_from_slice(&240u16.to_le_bytes());
    gif.extend_from_slice(&[0; 4]);
    let s = sniff_header(&gif).unwrap();
    assert_eq!((s.kind, s.width, s.height), (ImageKind::Gif, 320, 240));

    let mut bmp = vec![b'B', b'M'];
    bmp.extend_from_slice(&[0u8; 16]); // up to offset 18
    bmp.extend_from_slice(&800i32.to_le_bytes());
    bmp.extend_from_slice(&600i32.to_le_bytes());
    let s = sniff_header(&bmp).unwrap();
    assert_eq!((s.kind, s.width, s.height), (ImageKind::Bmp, 800, 600));
    assert!(!s.kind.decodable());

    let mut webp = b"RIFF\x00\x00\x00\x00WEBPVP8X".to_vec();
    webp.extend_from_slice(&[0u8; 4]); // flags
    webp.extend_from_slice(&127u32.to_le_bytes()[..3]);
    webp.extend_from_slice(&63u32.to_le_bytes()[..3]);
    let s = sniff_header(&webp).unwrap();
    assert_eq!((s.kind, s.width, s.height), (ImageKind::WebP, 128, 64));
}

#[test]
fn sniff_errors_on_truncated_and_garbage() {
    assert_eq!(sniff_header(&[0xFF, 0xD8]), Err(SniffError::Truncated));
    assert_eq!(sniff_header(b"not an image"), Err(SniffError::Unknown));
    // JPEG marker walk that ends before SOF.
    let v = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00];
    assert_eq!(sniff_header(&v), Err(SniffError::Truncated));
    // PNG with zero dims.
    let bad = minimal_png(0, 10);
    assert_eq!(sniff_header(&bad), Err(SniffError::Unknown));
}

#[test]
fn sniff_real_bench_photos() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tmp-bench-media");
    for (name, w, h) in [
        ("photo-4000x3000.jpg", 4000u32, 3000u32),
        ("photo-4917x3456.jpg", 4917, 3456),
        ("photo-3840x2880.png", 3840, 2880),
    ] {
        let p = format!("{dir}/{name}");
        let Ok(mut f) = std::fs::File::open(&p) else {
            println!("SKIP sniff_real_bench_photos: missing {p}");
            continue;
        };
        use std::io::Read;
        let mut head = vec![0u8; 256 * 1024];
        let n = f.read(&mut head).unwrap_or(0);
        let s = sniff_header(&head[..n]).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_eq!((s.width, s.height), (w, h), "{name}");
    }
}
