//! Image header sniffing: format classification + dimensions from the first
//! bytes of a file, without decoding pixel data.
//!
//! This powers three things in the thumbnail engine:
//! 1. admission-control pricing (decoded size estimate before committing),
//! 2. the small-image passthrough decision,
//! 3. decompression-bomb rejection (absurd dimensions → refuse early).

/// Image formats the engine can classify from a header alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Jpeg,
    Png,
    Gif,
    WebP,
    Bmp,
}

impl ImageKind {
    /// MIME type used when raw source bytes are served as-is.
    pub fn mime(self) -> &'static str {
        match self {
            ImageKind::Jpeg => "image/jpeg",
            ImageKind::Png => "image/png",
            ImageKind::Gif => "image/gif",
            ImageKind::WebP => "image/webp",
            ImageKind::Bmp => "image/bmp",
        }
    }

    /// Whether the in-tree decoders can decode this format to pixels
    /// (BMP can be classified but is not an enabled `image` feature).
    pub fn decodable(self) -> bool {
        !matches!(self, ImageKind::Bmp)
    }
}

/// Header facts extracted without pixel decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sniffed {
    pub kind: ImageKind,
    pub width: u32,
    pub height: u32,
    /// JPEG-only: progressive DCT scan order.
    pub progressive: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SniffError {
    /// Fewer bytes than needed for classification.
    Truncated,
    /// Not a recognized image header.
    Unknown,
}

impl std::fmt::Display for SniffError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SniffError::Truncated => write!(f, "truncated image header"),
            SniffError::Unknown => write!(f, "unrecognized image header"),
        }
    }
}

impl std::error::Error for SniffError {}

fn be16(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?]))
}
fn le16(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(i)?, *b.get(i + 1)?]))
}

/// Classify a header buffer. Only the leading bytes are required; passing the
/// whole file is fine but wasteful (`read_head` caps at 256 KiB).
pub fn sniff_header(buf: &[u8]) -> Result<Sniffed, SniffError> {
    if buf.len() < 12 {
        return Err(SniffError::Truncated);
    }
    if buf.starts_with(&[0xFF, 0xD8]) {
        return sniff_jpeg(buf);
    }
    if buf.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        // IHDR must be the first chunk: length(4) + "IHDR" then BE w/h.
        let w = u32::from_be_bytes([buf[16], buf[17], buf[18], buf[19]]);
        let h = u32::from_be_bytes([buf[20], buf[21], buf[22], buf[23]]);
        if w == 0 || h == 0 {
            return Err(SniffError::Unknown);
        }
        return Ok(Sniffed {
            kind: ImageKind::Png,
            width: w,
            height: h,
            progressive: false,
        });
    }
    if buf.starts_with(b"GIF87a") || buf.starts_with(b"GIF89a") {
        let w = u32::from(le16(buf, 6).ok_or(SniffError::Truncated)?);
        let h = u32::from(le16(buf, 8).ok_or(SniffError::Truncated)?);
        if w == 0 || h == 0 {
            return Err(SniffError::Unknown);
        }
        return Ok(Sniffed {
            kind: ImageKind::Gif,
            width: w,
            height: h,
            progressive: false,
        });
    }
    if buf.starts_with(b"RIFF") && buf.get(8..12) == Some(b"WEBP") {
        return sniff_webp(buf);
    }
    if buf.starts_with(b"BM") {
        // BITMAPFILEHEADER: width/height are signed i32 LE at offsets 18/22.
        let rd = |i: usize| -> Option<i32> {
            let b = buf.get(i..i + 4)?;
            Some(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        };
        let w = rd(18).ok_or(SniffError::Truncated)?;
        let h = rd(22).ok_or(SniffError::Truncated)?;
        let (w, h) = (w.unsigned_abs(), h.unsigned_abs());
        if w == 0 || h == 0 {
            return Err(SniffError::Unknown);
        }
        return Ok(Sniffed {
            kind: ImageKind::Bmp,
            width: w,
            height: h,
            progressive: false,
        });
    }
    Err(SniffError::Unknown)
}

/// Walk JPEG marker segments until a SOF frame header yields the dimensions.
fn sniff_jpeg(buf: &[u8]) -> Result<Sniffed, SniffError> {
    let sof =
        |m: u8| -> bool { matches!(m, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) };
    let mut i = 2usize;
    while i < buf.len() {
        if buf[i] != 0xFF {
            // Tolerate garbage between segments by scanning to the next marker.
            i += 1;
            continue;
        }
        // Skip fill bytes (repeated 0xFF padding before the marker code).
        let mut m_pos = i + 1;
        while m_pos < buf.len() && buf[m_pos] == 0xFF {
            m_pos += 1;
        }
        let Some(&m) = buf.get(m_pos) else {
            return Err(SniffError::Truncated);
        };
        match m {
            // Standalone markers without a length payload.
            0xD8 | 0x01 | 0xD0..=0xD7 => {
                i = m_pos + 1;
                continue;
            }
            _ if sof(m) => {
                // Layout after the marker code: len(2) precision(1)
                // height(2) width(2).
                let h = u32::from(be16(buf, m_pos + 4).ok_or(SniffError::Truncated)?);
                let w = u32::from(be16(buf, m_pos + 6).ok_or(SniffError::Truncated)?);
                if w == 0 || h == 0 {
                    return Err(SniffError::Unknown);
                }
                return Ok(Sniffed {
                    kind: ImageKind::Jpeg,
                    width: w,
                    height: h,
                    progressive: m == 0xC2,
                });
            }
            _ => {
                // Length-prefixed segment (APPn, DQT, DHT, ...): the 2-byte
                // BE length counts itself; skip to the next marker.
                let len = usize::from(be16(buf, m_pos + 1).ok_or(SniffError::Truncated)?);
                if len < 2 {
                    return Err(SniffError::Unknown);
                }
                i = m_pos + len + 1;
            }
        }
    }
    Err(SniffError::Truncated)
}

fn sniff_webp(buf: &[u8]) -> Result<Sniffed, SniffError> {
    // Chunk data starts at offset 12: "VP8X" | "VP8 " | "VP8L".
    match buf.get(12..16) {
        Some(b"VP8X") => {
            // VP8X: [flags(4)][canvas_w-1 (3B LE)][canvas_h-1 (3B LE)] at +4.
            let r = buf.get(20..26).ok_or(SniffError::Truncated)?;
            let w = 1u32 + (r[0] as u32 | (r[1] as u32) << 8 | (r[2] as u32) << 16);
            let h = 1u32 + (r[3] as u32 | (r[4] as u32) << 8 | (r[5] as u32) << 16);
            Ok(Sniffed {
                kind: ImageKind::WebP,
                width: w,
                height: h,
                progressive: false,
            })
        }
        Some(b"VP8 ") => {
            // Lossy: keyframe header; w/h are 14-bit LE values at offsets
            // 26/28 within the frame (after the 20-byte frame tag + sync code).
            let w = le16(buf, 26).ok_or(SniffError::Truncated)? & 0x3FFF;
            let h = le16(buf, 28).ok_or(SniffError::Truncated)? & 0x3FFF;
            if w == 0 || h == 0 {
                return Err(SniffError::Unknown);
            }
            Ok(Sniffed {
                kind: ImageKind::WebP,
                width: u32::from(w),
                height: u32::from(h),
                progressive: false,
            })
        }
        Some(b"VP8L") => {
            // Lossless: 0x2F signature, then 14-bit packed dims at offset 21:
            // bits [0..14) = width-1, bits [14..28) = height-1 (LE).
            let g = |r: usize| -> Option<u32> { buf.get(21 + r).map(|v| *v as u32) };
            let bits = |shift: u32, mask: u32| -> Option<u32> {
                let v = (g(0)? | (g(1)? << 8) | (g(2)? << 16) | (g(3)? << 24)) >> shift;
                Some(v & mask)
            };
            let w = bits(0, 0x3FFF).ok_or(SniffError::Truncated)? + 1;
            let h = bits(14, 0x3FFF).ok_or(SniffError::Truncated)? + 1;
            Ok(Sniffed {
                kind: ImageKind::WebP,
                width: w,
                height: h,
                progressive: false,
            })
        }
        _ => Err(SniffError::Unknown),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/media/thumb/sniff.rs"]
mod tests;
