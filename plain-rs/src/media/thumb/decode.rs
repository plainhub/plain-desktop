//! Decoding: dual JPEG decoder strategy + `image`-crate fallback formats.
//!
//! - Big-to-small JPEG (ratio ≥ 2): `jpeg-decoder` with IDCT-scaled decode
//!   (1/8, 1/4, 1/2) — the full-resolution bitmap never materializes, which
//!   is both the dominant latency win and the dominant memory win.
//! - Everything else (near-full-size JPEG, PNG, WebP, GIF, embedded covers):
//!   the `image` crate (zune-jpeg / image-webp / png backends).

use super::scale::Bitmap;
use anyhow::{Result, bail};

/// Largest JPEG dimension accepted by `Decoder::scale` (u16 API).
const SCALE_DIM_MAX: u32 = 65_535;

/// Predict the output dimensions `jpeg_decoder::Decoder::scale` will pick
/// for a source of (w, h) with target box (tw, th).
///
/// Mirrors the crate's rule: the smallest supported scale factor (most
/// aggressive reduction: 1/8 first) whose result is ≥ the requested size in
/// at least one axis. Used for admission pricing *before* decoding and
/// asserted against real decoder output in benchmarks/tests.
pub fn predict_scaled(w: u32, h: u32, tw: u32, th: u32) -> (u32, u32) {
    for f in [8u32, 4, 2, 1] {
        let sw = w.div_ceil(f);
        let sh = h.div_ceil(f);
        if sw >= tw || sh >= th {
            return (sw, sh);
        }
    }
    (w, h)
}

/// Decode a JPEG file directly at reduced resolution (IDCT scaling).
/// `tw`/`th` describe the *final* thumbnail box; the decoder returns the
/// nearest supported scale ≥ that box.
pub fn jpeg_scaled(path: &std::path::Path, tw: u32, th: u32) -> Result<Bitmap> {
    let file = std::fs::File::open(path)?;
    jpeg_scaled_reader(std::io::BufReader::new(file), tw, th)
}

/// Same as [`jpeg_scaled`] for in-memory JPEG bytes (embedded covers).
pub fn jpeg_scaled_bytes(data: &[u8], tw: u32, th: u32) -> Result<Bitmap> {
    jpeg_scaled_reader(std::io::Cursor::new(data), tw, th)
}

fn jpeg_scaled_reader<R: std::io::Read>(reader: R, tw: u32, th: u32) -> Result<Bitmap> {
    let mut dec = jpeg_decoder::Decoder::new(reader);
    dec.read_info()?;
    let info = dec
        .info()
        .ok_or_else(|| anyhow::anyhow!("jpeg-decoder gave no info after read_info"))?;
    let (sw, sh) = (u32::from(info.width), u32::from(info.height));
    let (rw, rh) = clamp_request(sw, sh, tw, th);
    dec.scale(rw as u16, rh as u16)?;
    let pixels = dec.decode()?;
    let info = dec
        .info()
        .ok_or_else(|| anyhow::anyhow!("jpeg-decoder gave no info after decode"))?;
    let (w, h) = (u32::from(info.width), u32::from(info.height));

    let bitmap = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => Bitmap::new_rgb(pixels, w, h),
        jpeg_decoder::PixelFormat::L8 => {
            let mut rgb = Vec::with_capacity(pixels.len() * 3);
            for v in &pixels {
                rgb.extend_from_slice(&[*v, *v, *v]);
            }
            Bitmap::new_rgb(rgb, w, h)
        }
        jpeg_decoder::PixelFormat::L16 => {
            let mut rgb = Vec::with_capacity(pixels.len() / 2 * 3);
            for p in pixels.chunks_exact(2) {
                let v = (u16::from_le_bytes([p[0], p[1]]) >> 8) as u8;
                rgb.extend_from_slice(&[v, v, v]);
            }
            Bitmap::new_rgb(rgb, w, h)
        }
        jpeg_decoder::PixelFormat::CMYK32 => {
            // Adobe JPEGs store inverted CMYK: 0 = full ink, 255 = paper.
            // Standard conversion: R = C·K/255 (per channel).
            let mut rgb = Vec::with_capacity(pixels.len() / 4 * 3);
            for p in pixels.chunks_exact(4) {
                let k = p[3] as u32;
                rgb.push((p[0] as u32 * k / 255) as u8);
                rgb.push((p[1] as u32 * k / 255) as u8);
                rgb.push((p[2] as u32 * k / 255) as u8);
            }
            Bitmap::new_rgb(rgb, w, h)
        }
    };
    Ok(bitmap)
}

/// Clamp the scale() request into u16 range while keeping the aspect hint.
fn clamp_request(sw: u32, sh: u32, tw: u32, th: u32) -> (u32, u32) {
    let mut rw = tw.clamp(1, SCALE_DIM_MAX);
    let mut rh = th.clamp(1, SCALE_DIM_MAX);
    // Never request more than the source itself: scale() must not upscale.
    if rw > sw || rh > sh {
        // Match the dominant axis so the 1/1 bucket is chosen naturally.
        if sw >= sh {
            rw = rw.min(sw);
            rh = rh.min(sh.max(1));
        } else {
            rh = rh.min(sh);
            rw = rw.min(sw.max(1));
        }
    }
    (rw, rh)
}

/// Full decode through the `image` crate (JPEG near-full-size, PNG, WebP,
/// GIF, BMP-less). Returns a Bitmap without intermediate DynamicImage
/// copies for the common RGB/RGBA cases.
pub fn image_full(path: &std::path::Path) -> Result<Bitmap> {
    let mut reader = image::ImageReader::open(path)?;
    reader = reader.with_guessed_format()?;
    reader.no_limits();
    let img = reader.decode().map_err(anyhow::Error::from)?;
    Ok(Bitmap::from_dynamic(img))
}

/// Decode from in-memory bytes (embedded cover art and sidecar covers).
pub fn image_from_bytes(data: &[u8]) -> Result<Bitmap> {
    if data.len() < 12 {
        bail!("cover bytes too short");
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(data));
    reader = reader.with_guessed_format()?;
    reader.no_limits();
    let img = reader.decode().map_err(anyhow::Error::from)?;
    Ok(Bitmap::from_dynamic(img))
}

#[cfg(test)]
#[path = "../../../tests/unit/media/thumb/decode.rs"]
mod tests;
