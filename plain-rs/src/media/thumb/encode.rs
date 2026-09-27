//! Thumbnail output encoding: pure-Rust baseline JPEG via `jpeg-encoder`.
//!
//! JPEG (quality < 90 defaults to 4:2:0 subsampling) is ~3× faster to encode
//! than lossy WebP at comparable visual quality, needs no C library, and is
//! universally renderable. Alpha is composited onto white first — JPEG has
//! no alpha channel.

use super::scale::Bitmap;
use anyhow::{Result, bail};

/// Encode a bitmap as JPEG. `quality` is clamped to 1..=100.
pub fn encode_jpeg(bm: Bitmap, quality: u8) -> Result<Vec<u8>> {
    let (w, h) = (bm.width(), bm.height());
    if w == 0 || h == 0 || w > u32::from(u16::MAX) || h > u32::from(u16::MAX) {
        bail!("jpeg encode dimensions out of range: {w}x{h}");
    }
    let quality = quality.clamp(1, 100);
    let bm = bm.flatten_to_rgb();
    let Bitmap::Rgb8 { data, .. } = &bm else {
        unreachable!("flatten_to_rgb always yields Rgb8");
    };

    let mut out = Vec::with_capacity(4096);
    let encoder = jpeg_encoder::Encoder::new(&mut out, quality);
    encoder.encode(data, w as u16, h as u16, jpeg_encoder::ColorType::Rgb)?;
    Ok(out)
}

#[cfg(test)]
#[path = "../../../tests/unit/media/thumb/encode.rs"]
mod tests;
