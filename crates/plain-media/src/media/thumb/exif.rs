//! Minimal EXIF orientation reader for JPEG (APP1 segment only).
//!
//! Full EXIF parsing is out of scope; we only need IFD0 tag `0x0112`
//! (Orientation) so generated thumbnails match how phones/cameras display
//! the photo. Runs on the already-read header buffer — zero extra I/O.

/// EXIF orientation value (1 = normal, 2..8 = mirrored/rotated).
/// `0` is treated as absent/normal by callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Orientation(pub u8);

impl Orientation {
    pub const NORMAL: Orientation = Orientation(1);

    /// Whether the stored image needs its axes swapped to display upright.
    pub fn swaps_axes(self) -> bool {
        matches!(self.0, 5..=8)
    }
}

impl Default for Orientation {
    fn default() -> Self {
        Self::NORMAL
    }
}

/// Extract the orientation from a JPEG header buffer (leading bytes of the
/// file, at least through the APP1 segment). Returns `None` when there is no
/// usable EXIF orientation (missing APP1, non-TIFF payload, tag absent).
pub fn orientation(jpeg_head: &[u8]) -> Option<u8> {
    let mut i = 2usize;
    while i + 4 < jpeg_head.len() {
        if jpeg_head[i] != 0xFF {
            i += 1;
            continue;
        }
        let m_pos = i + 1;
        let m = *jpeg_head.get(m_pos)?;
        match m {
            0xD8 | 0x01 | 0xD0..=0xD7 => {
                i = m_pos + 1;
                continue;
            }
            0xE1 => {
                // APP1: check "Exif\0\0" then parse the TIFF header.
                let len =
                    u16::from_be_bytes([*jpeg_head.get(m_pos + 1)?, *jpeg_head.get(m_pos + 2)?])
                        as usize;
                let payload = jpeg_head.get(m_pos + 3..(m_pos + 1 + len).min(jpeg_head.len()))?;
                if payload.len() < 8 || &payload[..6] != b"Exif\0\0" {
                    return None; // not an EXIF APP1 (e.g. XMP)
                }
                return tiff_orientation(&payload[6..]);
            }
            0xDA => return None, // start of scan: no APP1 found
            _ => {
                let len =
                    u16::from_be_bytes([*jpeg_head.get(m_pos + 1)?, *jpeg_head.get(m_pos + 2)?])
                        as usize;
                if len < 2 {
                    return None;
                }
                i = m_pos + len + 1;
            }
        }
    }
    None
}

/// Parse a TIFF blob (big/little endian IFD0) for the orientation tag.
fn tiff_orientation(tiff: &[u8]) -> Option<u8> {
    if tiff.len() < 8 {
        return None;
    }
    let le = match &tiff[..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let rd16 = |b: &[u8], i: usize| -> Option<u16> {
        let p = b.get(i..i + 2)?;
        Some(if le {
            u16::from_le_bytes([p[0], p[1]])
        } else {
            u16::from_be_bytes([p[0], p[1]])
        })
    };
    let rd32 = |b: &[u8], i: usize| -> Option<u32> {
        let p = b.get(i..i + 4)?;
        Some(if le {
            u32::from_le_bytes([p[0], p[1], p[2], p[3]])
        } else {
            u32::from_be_bytes([p[0], p[1], p[2], p[3]])
        })
    };
    if rd16(tiff, 2)? != 42 {
        return None;
    }
    let ifd0 = rd32(tiff, 4)? as usize;
    let count = rd16(tiff, ifd0)? as usize;
    for e in 0..count {
        let entry = ifd0 + 2 + e * 12;
        let tag = rd16(tiff, entry)?;
        if tag != 0x0112 {
            continue;
        }
        // type must be SHORT (3) with count 1 for a sane orientation field.
        if rd16(tiff, entry + 2)? != 3 || rd32(tiff, entry + 4)? != 1 {
            return None;
        }
        let v = rd16(tiff, entry + 8)?;
        return if (1..=8).contains(&v) {
            Some(v as u8)
        } else {
            None
        };
    }
    None
}

/// Build a synthetic APP1 segment carrying the given orientation. Used by
/// tests to produce JPEGs with a known EXIF orientation.
#[cfg(test)]
pub(crate) fn test_app1_orientation(v: u8) -> Vec<u8> {
    let mut tiff: Vec<u8> = b"II".to_vec();
    tiff.extend_from_slice(&42u16.to_le_bytes());
    tiff.extend_from_slice(&8u32.to_le_bytes()); // IFD0 at offset 8
    tiff.extend_from_slice(&1u16.to_le_bytes()); // 1 entry
    tiff.extend_from_slice(&0x0112u16.to_le_bytes()); // tag
    tiff.extend_from_slice(&3u16.to_le_bytes()); // SHORT
    tiff.extend_from_slice(&1u32.to_le_bytes()); // count
    tiff.extend_from_slice(&(v as u16).to_le_bytes());
    tiff.extend_from_slice(&[0, 0]); // value padding
    tiff.extend_from_slice(&0u32.to_be_bytes()); // next IFD = 0

    let mut app1 = vec![0xFF, 0xE1];
    let payload_len = 6 + tiff.len(); // "Exif\0\0" + TIFF
    app1.extend_from_slice(&((payload_len + 2) as u16).to_be_bytes());
    app1.extend_from_slice(b"Exif\0\0");
    app1.extend_from_slice(&tiff);
    app1
}

#[cfg(test)]
#[path = "../../../tests/unit/media/thumb/exif.rs"]
mod tests;
