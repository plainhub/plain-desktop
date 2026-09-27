//! Thumbnail pipeline benchmark CLI — self-contained phase comparison.
//!
//! Usage:
//!   cargo run --release --bin thumb_bench -- <image_path> [target] [iters]
//!
//! Measures, on a real file: IDCT-scaled JPEG decode (jpeg-decoder `scale`)
//! vs full decode (image/zune-jpeg), box+SIMD resize, and pure-Rust JPEG
//! encode. This replaces the old libwebp strategy bench when WebP output
//! was removed in favor of the pure-Rust pipeline.
//!
//! Self-contained by necessity: bins are separate crates and the engine
//! lives in the main binary, so the pipeline stages are mirrored here.

use std::time::{Duration, Instant};

/// Minimal bitmap: flat RGB8.
struct Bm {
    data: Vec<u8>,
    w: u32,
    h: u32,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).cloned().unwrap_or_else(|| {
        eprintln!("usage: thumb_bench <image_path> [target=512] [iters=5]");
        std::process::exit(2);
    });
    let target: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(512);
    let iters: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(5);

    let meta = std::fs::metadata(&path).expect("stat image");
    let head = std::fs::read(&path).unwrap_or_default();
    let head = &head[..head.len().min(256 * 1024)];

    let (w, h, is_jpeg) = sniff_dims(head).expect("unrecognized image header");
    let (tw, th) = target_size(w, h, target, target);
    println!("=== Thumbnail Pipeline Benchmark ===");
    println!(
        "Image:  {path} ({w}x{h}, {:.1} MB, jpeg={is_jpeg}, progressive={:?})",
        meta.len() as f64 / 1024.0 / 1024.0,
        is_progressive(head),
    );
    println!("Target: {tw}x{th}  iters: {iters}  threads: {}", threads());

    // Warm page cache + allocator once.
    let _ = full_decode(&path);

    let mut dec_scl = Vec::new();
    let mut dec_full = Vec::new();
    let mut dec_jd_full = Vec::new();
    let mut rsz_from_full = Vec::new();
    let mut enc_t = Vec::new();
    for _ in 0..iters {
        if is_jpeg && w >= tw * 2 {
            let t = Instant::now();
            decode_scaled(&path, tw, th).expect("scaled decode");
            dec_scl.push(t.elapsed());
        }
        let t = Instant::now();
        let img = full_decode(&path);
        dec_full.push(t.elapsed());

        if is_jpeg {
            let t = Instant::now();
            decode_jd_full(&path).expect("jpeg-decoder full");
            dec_jd_full.push(t.elapsed());
        }

        // Resize the full bitmap through the engine path (box + fir).
        let t = Instant::now();
        let thumb = resize_to(img, tw, th);
        rsz_from_full.push(t.elapsed());

        let t = Instant::now();
        encode_jpeg(&thumb, 75);
        enc_t.push(t.elapsed());
    }

    let med = |mut v: Vec<Duration>| {
        v.sort();
        v[v.len() / 2]
    };
    println!();
    if !dec_scl.is_empty() {
        let s = med(dec_scl.clone());
        let f = med(dec_full.clone());
        println!(
            "median decode: scaled {s:?} vs full {f:?} → {:.1}× faster",
            f.as_secs_f64() / s.as_secs_f64()
        );
    } else {
        println!("median full decode: {:?}", med(dec_full.clone()));
    }
    if !dec_jd_full.is_empty() {
        println!(
            "median jpeg-decoder full: {:?} (vs zune {:?})",
            med(dec_jd_full),
            med(dec_full)
        );
    }
    println!(
        "median resize (box+fir, from full): {:?}",
        med(rsz_from_full)
    );
    println!("median encode: {:?}", med(enc_t));

    // End-to-end comparison of both decode paths + resize + encode.
    if !dec_scl.is_empty() {
        let mut e2e_scl = Vec::new();
        let mut e2e_full = Vec::new();
        for _ in 0..iters {
            let t = Instant::now();
            let bm = decode_scaled_bm(&path, tw, th).expect("scaled decode");
            let thumb = resize_to(bm, tw, th);
            encode_jpeg(&thumb, 75);
            e2e_scl.push(t.elapsed());

            let t = Instant::now();
            let bm = full_decode(&path);
            let thumb = resize_to(bm, tw, th);
            encode_jpeg(&thumb, 75);
            e2e_full.push(t.elapsed());
        }
        println!(
            "median end-to-end: scaled {:?} vs full {:?}",
            med(e2e_scl),
            med(e2e_full)
        );
    }
}

fn threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

fn full_decode(path: &str) -> Bm {
    let mut reader = image::ImageReader::open(path)
        .unwrap()
        .with_guessed_format()
        .unwrap();
    reader.no_limits();
    let img = reader.decode().expect("full decode").to_rgb8();
    let (w, h) = (img.width(), img.height());
    Bm {
        data: img.into_raw(),
        w,
        h,
    }
}

fn decode_scaled(path: &str, tw: u32, th: u32) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    let mut dec = jpeg_decoder::Decoder::new(std::io::BufReader::new(file));
    dec.read_info().ok()?;
    dec.scale(tw as u16, th as u16).ok()?;
    dec.decode().ok()
}

/// Scaled decode into the bench bitmap shape.
fn decode_scaled_bm(path: &str, tw: u32, th: u32) -> Option<Bm> {
    let file = std::fs::File::open(path).ok()?;
    let mut dec = jpeg_decoder::Decoder::new(std::io::BufReader::new(file));
    dec.read_info().ok()?;
    dec.scale(tw as u16, th as u16).ok()?;
    let pixels = dec.decode().ok()?;
    let info = dec.info()?;
    let mut data = Vec::with_capacity(pixels.len() / 4 * 3);
    match info.pixel_format {
        jpeg_decoder::PixelFormat::CMYK32 => {
            for p in pixels.chunks_exact(4) {
                let k = p[3] as u32;
                data.push((p[0] as u32 * k / 255) as u8);
                data.push((p[1] as u32 * k / 255) as u8);
                data.push((p[2] as u32 * k / 255) as u8);
            }
        }
        _ => {
            for p in pixels.chunks_exact(3) {
                data.extend_from_slice(&p[..3]);
            }
        }
    }
    Some(Bm {
        w: u32::from(info.width),
        h: u32::from(info.height),
        data,
    })
}

fn decode_jd_full(path: &str) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    let mut dec = jpeg_decoder::Decoder::new(std::io::BufReader::new(file));
    dec.decode().ok()
}

// ── Engine pipeline mirrors (scale.rs / encode.rs) ─────────────────────────

fn box_downscale(bm: &Bm, f: u32) -> Bm {
    let (src, w, h) = (&bm.data, bm.w, bm.h);
    let ch = 3usize;
    let (ow, oh) = (w.div_ceil(f), h.div_ceil(f));

    let vw = w as usize;
    let mut vert = vec![0u32; vw * oh as usize * ch];
    for oy in 0..oh as usize {
        let row = &mut vert[oy * vw * ch..(oy + 1) * vw * ch];
        let y0 = oy * f as usize;
        let y1 = ((oy + 1) * f as usize).min(h as usize);
        for y in y0..y1 {
            let srow = &src[y * vw * ch..(y + 1) * vw * ch];
            for (acc, v) in row.iter_mut().zip(srow) {
                *acc += *v as u32;
            }
        }
        let n = (y1 - y0) as u32;
        for acc in row.iter_mut() {
            *acc = (*acc + n / 2) / n;
        }
    }

    let mut dst = vec![0u8; ow as usize * oh as usize * ch];
    for oy in 0..oh as usize {
        let vrow = &vert[oy * vw * ch..(oy + 1) * vw * ch];
        let orow = &mut dst[oy * ow as usize * ch..(oy + 1) * ow as usize * ch];
        let mut acc = vec![0u32; ch];
        for ox in 0..ow as usize {
            acc.iter_mut().for_each(|a| *a = 0);
            let x0 = ox * f as usize;
            let x1 = ((ox + 1) * f as usize).min(w as usize);
            let n = (x1 - x0) as u32;
            for x in x0..x1 {
                for c in 0..ch {
                    acc[c] += vrow[x * ch + c];
                }
            }
            for c in 0..ch {
                orow[ox * ch + c] = ((acc[c] + n / 2) / n).min(255) as u8;
            }
        }
    }
    Bm {
        data: dst,
        w: ow,
        h: oh,
    }
}

fn resize_to(bm: Bm, tw: u32, th: u32) -> Bm {
    let (w, h) = (bm.w, bm.h);
    if tw == 0 || th == 0 || (tw == w && th == h) {
        return bm;
    }
    let f = w
        .checked_div(tw)
        .unwrap_or(0)
        .min(h.checked_div(th).unwrap_or(0))
        .min(64);
    let mid = if f >= 2 && (w / f >= tw && h / f >= th) {
        box_downscale(&bm, f)
    } else {
        bm
    };
    fir_resize(mid, tw, th)
}

fn fir_resize(bm: Bm, tw: u32, th: u32) -> Bm {
    use fast_image_resize::{PixelType, ResizeAlg, ResizeOptions, Resizer};
    let (w, h) = (bm.w, bm.h);
    if tw == w && th == h {
        return bm;
    }
    let src =
        fast_image_resize::images::Image::from_vec_u8(w, h, bm.data, PixelType::U8x3).unwrap();
    let mut dst = fast_image_resize::images::Image::new(tw, th, PixelType::U8x3);
    let mut resizer = Resizer::new();
    let filter = if tw.max(th) < 200 {
        fast_image_resize::FilterType::Bilinear
    } else {
        fast_image_resize::FilterType::Lanczos3
    };
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(filter));
    resizer.resize(&src, &mut dst, &options).unwrap();
    Bm {
        data: dst.into_vec(),
        w: tw,
        h: th,
    }
}

fn encode_jpeg(bm: &Bm, quality: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(4096);
    let encoder = jpeg_encoder::Encoder::new(&mut out, quality);
    encoder
        .encode(
            &bm.data,
            bm.w as u16,
            bm.h as u16,
            jpeg_encoder::ColorType::Rgb,
        )
        .expect("bench encode");
    out
}

// ── Header sniffing ────────────────────────────────────────────────────────

/// Minimal JPEG/PNG/GIF/WebP header sniffer (bench-only; the engine has the
/// full version with tests).
fn sniff_dims(b: &[u8]) -> Option<(u32, u32, bool)> {
    if b.starts_with(&[0xFF, 0xD8]) {
        let mut i = 2;
        while i + 9 < b.len() {
            if b[i] != 0xFF {
                i += 1;
                continue;
            }
            let m = b[i + 1];
            if matches!(m, 0xC0..=0xCF) && !matches!(m, 0xC4 | 0xC8 | 0xCC) {
                let h = u16::from_be_bytes([b[i + 5], b[i + 6]]);
                let w = u16::from_be_bytes([b[i + 7], b[i + 8]]);
                return Some((u32::from(w), u32::from(h), true));
            }
            let len = usize::from(u16::from_be_bytes([b[i + 2], b[i + 3]]));
            i += 2 + len;
        }
        return None;
    }
    if b.starts_with(&[0x89, b'P', b'N', b'G']) && b.len() >= 24 {
        let w = u32::from_be_bytes([b[16], b[17], b[18], b[19]]);
        let h = u32::from_be_bytes([b[20], b[21], b[22], b[23]]);
        return Some((w, h, false));
    }
    None
}

/// SOF2 (0xC2) = progressive; SOF0/1 = baseline.
fn is_progressive(b: &[u8]) -> Option<bool> {
    if !b.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut i = 2;
    while i + 9 < b.len() {
        if b[i] != 0xFF {
            i += 1;
            continue;
        }
        let m = b[i + 1];
        if matches!(m, 0xC0..=0xCF) && !matches!(m, 0xC4 | 0xC8 | 0xCC) {
            return Some(matches!(m, 0xC2 | 0xC6 | 0xCA));
        }
        let len = usize::from(u16::from_be_bytes([b[i + 2], b[i + 3]]));
        i += 2 + len;
    }
    None
}

fn target_size(w: u32, h: u32, bw: u32, bh: u32) -> (u32, u32) {
    let r = f64::from(bw.min(bh)) / f64::from(w.max(h));
    if r >= 1.0 {
        (w, h)
    } else {
        (
            ((f64::from(w) * r) as u32).max(1),
            ((f64::from(h) * r) as u32).max(1),
        )
    }
}
