//! Hand-rolled image dimension parsing from raw bytes.
//!
//! Returns `(width, height)` for JPEG / PNG / GIF / BMP / WebP / ICO /
//! TIFF by reading the format's magic bytes and header fields directly —
//! mirrors `plain-app`'s `platform/getImageDimensions` (Android
//! `BitmapFactory` behaviour, including ICO where a `0` entry encodes
//! 256). Also provides [`exif_gps`], a hand-rolled EXIF GPS reader for
//! JPEG (APP1 `Exif\0\0`) and TIFF payloads. Anything undecodable yields
//! `None`.

/// Returns `(width, height)` for the image in `bytes`, or `None` when
/// `bytes` is not one of the supported formats.
pub fn dimensions(bytes: &[u8]) -> Option<(i32, i32)> {
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
        return jpeg(bytes);
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return png(bytes);
    }
    if bytes.len() >= 6 && (&bytes[..6] == b"GIF87a" || &bytes[..6] == b"GIF89a") {
        return gif(bytes);
    }
    if bytes.starts_with(b"BM") {
        return bmp(bytes);
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return webp(bytes);
    }
    // TIFF (little- or big-endian).
    if bytes.starts_with(b"II") || bytes.starts_with(b"MM") {
        return tiff(bytes);
    }
    // ICO/CUR container: reserved(0,0) + type(1=icon) + image count (>0).
    if bytes.len() >= 8 && bytes[0] == 0 && bytes[1] == 0 && bytes[2] == 1 {
        return ico(bytes);
    }
    None
}

/// Extract GPS coordinates from the EXIF metadata of a JPEG
/// (APP1 `Exif\0\0` segment) or a TIFF payload. Returns
/// `(latitude, longitude)` in signed decimal degrees, or `None` when the
/// image carries no usable GPS IFD. Ported from plain-desktop
/// `file_query.rs` (used for photo locations in chat / media info).
pub fn exif_gps(bytes: &[u8]) -> Option<(f64, f64)> {
    let tiff: &[u8] = if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
        exif_tiff_from_jpeg(bytes)?
    } else {
        bytes
    };
    let (le, ifd0_off) = parse_tiff_header(tiff)?;
    let gps_off = tiff_find_sub_ifd(tiff, ifd0_off, le, 0x8825)?;
    let lat = tiff_read_dms(tiff, gps_off, le, 0x0002, 0x0001)?;
    let lon = tiff_read_dms(tiff, gps_off, le, 0x0004, 0x0003)?;
    Some((lat, lon))
}

/// JPEG dimensions from the first SOF marker. Returns `None` when no
/// SOF marker is found before SOS / EOI.
fn jpeg(b: &[u8]) -> Option<(i32, i32)> {
    let mut i = 2usize;
    while i + 1 < b.len() {
        if b[i] != 0xFF {
            i += 1;
            continue;
        }
        // Markers can be repeated (0xFF 0xFF) — skip padding fills.
        while i < b.len() && b[i] == 0xFF {
            i += 1;
        }
        if i >= b.len() {
            return None;
        }
        let m = b[i];
        i += 1;
        // SOF markers (Start Of Frame), except 0xC4 (DHT) and 0xC8 (JPG).
        if (0xC0..=0xCF).contains(&m) && m != 0xC4 && m != 0xC8 {
            if i + 7 > b.len() {
                return None;
            }
            // Precision (1) + height (2) + width (2) follow the segment length.
            let h = u16::from_be_bytes([b[i + 3], b[i + 4]]) as i32;
            let w = u16::from_be_bytes([b[i + 5], b[i + 6]]) as i32;
            return Some((w, h));
        }
        // SOS (0xDA) or EOI (0xD9) — no more dimension markers ahead.
        if m == 0xDA || m == 0xD9 {
            return None;
        }
        if i + 2 > b.len() {
            return None;
        }
        let seg = u16::from_be_bytes([b[i], b[i + 1]]) as usize;
        if seg < 2 {
            return None;
        }
        i += 2 + (seg - 2);
    }
    None
}

/// PNG dimensions from the IHDR chunk.
fn png(b: &[u8]) -> Option<(i32, i32)> {
    if b.len() < 24 {
        return None;
    }
    if &b[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes(b[16..20].try_into().ok()?) as i32;
    let h = u32::from_be_bytes(b[20..24].try_into().ok()?) as i32;
    Some((w, h))
}

/// GIF dimensions (logical screen descriptor width/height).
fn gif(b: &[u8]) -> Option<(i32, i32)> {
    if b.len() < 10 {
        return None;
    }
    let w = u16::from_le_bytes([b[6], b[7]]) as i32;
    let h = u16::from_le_bytes([b[8], b[9]]) as i32;
    Some((w, h))
}

/// BMP dimensions (width / height at the DIB header offsets 18 / 22).
fn bmp(b: &[u8]) -> Option<(i32, i32)> {
    if b.len() < 26 {
        return None;
    }
    let w = i32::from_le_bytes(b[18..22].try_into().ok()?);
    let h = i32::from_le_bytes(b[22..26].try_into().ok()?);
    Some((w.abs(), h.abs()))
}

/// WebP dimensions across the three chunk types (`VP8 `, `VP8L`, `VP8X`).
fn webp(b: &[u8]) -> Option<(i32, i32)> {
    if b.len() < 16 {
        return None;
    }
    match &b[12..16] {
        b"VP8 " => {
            if b.len() < 30 {
                return None;
            }
            // Frame tag + start code (10 bytes), scale (3), width + height (4).
            let i = 16 + 10;
            let w = u16::from_le_bytes([b[i], b[i + 1]]) as i32 & 0x3FFF;
            let h = u16::from_le_bytes([b[i + 2], b[i + 3]]) as i32 & 0x3FFF;
            Some((w, h))
        }
        b"VP8L" => {
            if b.len() < 21 {
                return None;
            }
            let i = 16 + 1;
            let d = &b[i..i + 4];
            let w = 1 + (((d[1] & 0x3F) as i32) << 8 | d[0] as i32);
            let h = 1
                + ((((d[3] & 0x0F) as i32) << 10)
                    | ((d[2] as i32) << 2)
                    | ((d[1] & 0xC0) as i32) >> 6);
            Some((w, h))
        }
        b"VP8X" => {
            if b.len() < 30 {
                return None;
            }
            let i = 16 + 8;
            let d = &b[i..i + 6];
            let w = (d[0] as i32) | ((d[1] as i32) << 8) | ((d[2] as i32) << 16) + 1;
            let h = (d[3] as i32) | ((d[4] as i32) << 8) | ((d[5] as i32) << 16) + 1;
            Some((w, h))
        }
        _ => None,
    }
}

/// ICO dimensions from the first `ICONDIRENTRY` (`0` encodes 256).
/// Matches Android's `BitmapFactory` result for `.ico` favicons.
fn ico(b: &[u8]) -> Option<(i32, i32)> {
    let count = u16::from_le_bytes([b[4], b[5]]);
    if count == 0 {
        return None;
    }
    // First entry starts at offset 6; width / height are its first two bytes.
    let w = b[6] as i32;
    let h = b[7] as i32;
    Some((if w == 0 { 256 } else { w }, if h == 0 { 256 } else { h }))
}

// ── TIFF dimensions + EXIF GPS (ported from plain-desktop file_query.rs) ─────

fn tiff(b: &[u8]) -> Option<(i32, i32)> {
    let (le, ifd_off) = parse_tiff_header(b)?;
    let count = tiff_read_u16(b, ifd_off as usize, le)? as usize;
    let base = ifd_off as usize + 2;
    let mut width: Option<i32> = None;
    let mut height: Option<i32> = None;
    for i in 0..count {
        let entry = base + i * 12;
        if entry + 12 > b.len() {
            return None;
        }
        let tag = tiff_read_u16(b, entry, le)?;
        if tag == 0x0100 {
            width = tiff_ifd_value_i32(b, entry, le);
        } else if tag == 0x0101 {
            height = tiff_ifd_value_i32(b, entry, le);
        }
        if width.is_some() && height.is_some() {
            break;
        }
    }
    Some((width?, height?))
}

/// Parse the 8-byte TIFF header: byte order + magic 42 + first IFD offset.
fn parse_tiff_header(tiff: &[u8]) -> Option<(bool, u32)> {
    if tiff.len() < 8 {
        return None;
    }
    let (le, magic) = match &tiff[0..2] {
        b"II" => (true, 0x002A),
        b"MM" => (false, 0x002A),
        _ => return None,
    };
    let expected = if le {
        u16::from_le_bytes([tiff[2], tiff[3]])
    } else {
        u16::from_be_bytes([tiff[2], tiff[3]])
    };
    if expected != magic {
        return None;
    }
    let off = if le {
        u32::from_le_bytes([tiff[4], tiff[5], tiff[6], tiff[7]])
    } else {
        u32::from_be_bytes([tiff[4], tiff[5], tiff[6], tiff[7]])
    };
    Some((le, off))
}

fn tiff_read_u16(tiff: &[u8], off: usize, le: bool) -> Option<u16> {
    let b = tiff.get(off..off + 2)?;
    Some(if le {
        u16::from_le_bytes([b[0], b[1]])
    } else {
        u16::from_be_bytes([b[0], b[1]])
    })
}

fn tiff_read_u32(tiff: &[u8], off: usize, le: bool) -> Option<u32> {
    let b = tiff.get(off..off + 4)?;
    Some(if le {
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    } else {
        u32::from_be_bytes([b[0], b[1], b[2], b[3]])
    })
}

/// Decode a SHORT (type 3) or LONG (type 4) IFD value that fits inline in
/// the entry's 4-byte value field.
fn tiff_ifd_value_i32(tiff: &[u8], entry: usize, le: bool) -> Option<i32> {
    let typ = tiff_read_u16(tiff, entry + 2, le)?;
    match typ {
        3 => Some(tiff_read_u16(tiff, entry + 8, le)? as i32),
        4 => Some(tiff_read_u32(tiff, entry + 8, le)? as i32),
        _ => None,
    }
}

/// Find a sub-IFD (e.g. GPS IFD) by tag id in the IFD at `ifd_off`. Returns
/// the offset of the sub-IFD's first entry.
fn tiff_find_sub_ifd(tiff: &[u8], ifd_off: u32, le: bool, sub_tag: u16) -> Option<u32> {
    let count = tiff_read_u16(tiff, ifd_off as usize, le)? as usize;
    let base = ifd_off as usize + 2;
    for i in 0..count {
        let entry = base + i * 12;
        if entry + 12 > tiff.len() {
            return None;
        }
        let tag = tiff_read_u16(tiff, entry, le)?;
        if tag == sub_tag {
            // Type 4 (LONG) value is stored inline in the 4-byte value field.
            return tiff_read_u32(tiff, entry + 8, le);
        }
    }
    None
}

/// Read a GPS DMS coordinate (RATIONAL × 3) and apply the matching
/// `*Ref` (ASCII "N"/"S"/"E"/"W") tag to derive a signed decimal.
fn tiff_read_dms(tiff: &[u8], ifd_off: u32, le: bool, dms_tag: u16, ref_tag: u16) -> Option<f64> {
    let (dms, ref_marker) = tiff_find_dms_and_ref(tiff, ifd_off, le, dms_tag, ref_tag)?;
    let mut iter = dms.iter();
    let d = iter.next()?.to_f64();
    let m = iter.next()?.to_f64();
    let s = iter.next()?.to_f64();
    let mut decimal = d + m / 60.0 + s / 3600.0;
    if ref_marker.contains('S') || ref_marker.contains('W') {
        decimal = -decimal;
    }
    Some(decimal)
}

fn tiff_find_dms_and_ref(
    tiff: &[u8],
    ifd_off: u32,
    le: bool,
    dms_tag: u16,
    ref_tag: u16,
) -> Option<(Vec<TiffRational>, String)> {
    let count = tiff_read_u16(tiff, ifd_off as usize, le)? as usize;
    let base = ifd_off as usize + 2;
    let mut dms: Option<Vec<TiffRational>> = None;
    let mut ref_marker: Option<String> = None;
    for i in 0..count {
        let entry = base + i * 12;
        if entry + 12 > tiff.len() {
            return None;
        }
        let tag = tiff_read_u16(tiff, entry, le)?;
        let typ = tiff_read_u16(tiff, entry + 2, le)?;
        if tag == dms_tag && typ == 5 {
            let off = tiff_read_u32(tiff, entry + 8, le)? as usize;
            dms = Some(tiff_read_rationals(tiff, off, le, 3)?);
        } else if tag == ref_tag && typ == 2 {
            // ASCII: value is N bytes starting at `off` (or inline if
            // count ≤ 4). Use whichever is shorter.
            let count_bytes = tiff_read_u32(tiff, entry + 4, le)? as usize;
            let buf: &[u8] = if count_bytes <= 4 {
                &tiff[entry + 8..entry + 8 + count_bytes]
            } else {
                let off = tiff_read_u32(tiff, entry + 8, le)? as usize;
                tiff.get(off..off + count_bytes)?
            };
            ref_marker = Some(
                String::from_utf8_lossy(buf)
                    .trim_end_matches('\0')
                    .to_string(),
            );
        }
        if dms.is_some() && ref_marker.is_some() {
            break;
        }
    }
    Some((dms?, ref_marker.unwrap_or_default()))
}

fn tiff_read_rationals(
    tiff: &[u8],
    off: usize,
    le: bool,
    count: usize,
) -> Option<Vec<TiffRational>> {
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let p = off + i * 8;
        if p + 8 > tiff.len() {
            return None;
        }
        let n = tiff_read_u32(tiff, p, le)?;
        let d = tiff_read_u32(tiff, p + 4, le)?;
        out.push(TiffRational { n, d });
    }
    Some(out)
}

/// Hand-rolled RATIONAL: 32-bit unsigned numerator / denominator.
#[derive(Clone, Copy)]
struct TiffRational {
    n: u32,
    d: u32,
}

impl TiffRational {
    fn to_f64(self) -> f64 {
        if self.d == 0 {
            0.0
        } else {
            (self.n as f64) / (self.d as f64)
        }
    }
}

/// Find the `Exif\0\0` marker in a JPEG's APP1 segment and return the
/// TIFF payload that follows it. Returns `None` if there is no EXIF.
fn exif_tiff_from_jpeg(jpeg: &[u8]) -> Option<&[u8]> {
    if jpeg.len() < 4 || jpeg[0..2] != [0xFF, 0xD8] {
        return None;
    }
    let mut i = 2;
    while i + 4 <= jpeg.len() {
        if jpeg[i] != 0xFF {
            return None;
        }
        // Skip 0xFF padding.
        let mut marker = jpeg[i + 1];
        while marker == 0xFF && i + 2 < jpeg.len() {
            i += 1;
            marker = jpeg[i + 1];
        }
        i += 2;
        if marker == 0xD9 || marker == 0xDA {
            return None;
        }
        if i + 2 > jpeg.len() {
            return None;
        }
        let seg_len = u16::from_be_bytes([jpeg[i], jpeg[i + 1]]) as usize;
        if seg_len < 2 || i + seg_len > jpeg.len() {
            return None;
        }
        let payload = &jpeg[i + 2..i + seg_len];
        if marker == 0xE1 && payload.starts_with(b"Exif\0\0") {
            return Some(&payload[6..]);
        }
        i += seg_len;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{dimensions, exif_gps};

    #[test]
    fn ico_dimensions_parse() {
        // ICONDIR: reserved(0,0) | type(1) | count(2) | first entry w/h.
        let mut b = Vec::new();
        b.extend_from_slice(&[0x00, 0x00, 0x01, 0x00, 0x03, 0x00]); // header, 3 images
        b.extend_from_slice(&[0x10, 0x10]); // first entry: w=16, h=16
        assert_eq!(dimensions(&b), Some((16, 16)));
    }

    #[test]
    fn ico_zero_width_means_256() {
        let mut b = Vec::new();
        b.extend_from_slice(&[0x00, 0x00, 0x01, 0x00, 0x01, 0x00]);
        b.extend_from_slice(&[0x00, 0x00]); // 0 => 256
        assert_eq!(dimensions(&b), Some((256, 256)));
    }

    #[test]
    fn png_dimensions_parse() {
        let mut b = Vec::new();
        b.extend_from_slice(b"\x89PNG\r\n\x1a\n");
        b.extend_from_slice(&[0x00, 0x00, 0x00, 0x0D]); // IHDR length
        b.extend_from_slice(b"IHDR");
        b.extend_from_slice(&[0x00, 0x00, 0x01, 0x00]); // width = 256
        b.extend_from_slice(&[0x00, 0x00, 0x00, 0x40]); // height = 64
        assert_eq!(dimensions(&b), Some((256, 64)));
    }

    #[test]
    fn jpeg_dimensions_parse() {
        // FF D8 | FF C0 | seg(2) | precis(1) | h(2) | w(2)
        let b = [
            0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x00, 0x20, 0x01, 0x00,
        ];
        assert_eq!(dimensions(&b), Some((256, 32)));
    }

    #[test]
    fn gif_dimensions_parse() {
        let mut b = Vec::new();
        b.extend_from_slice(b"GIF89a");
        b.extend_from_slice(&[0xFF, 0x00, 0x40, 0x00]); // w=255, h=64 (LE)
        assert_eq!(dimensions(&b), Some((255, 64)));
    }

    #[test]
    fn bmp_dimensions_parse() {
        let mut b = Vec::new();
        b.extend_from_slice(b"BM");
        b.resize(18, 0);
        b.extend_from_slice(&[0x00, 0x02, 0x00, 0x00]); // width = 512 (LE)
        b.extend_from_slice(&[0x64, 0x00, 0x00, 0x00]); // height = 100 (LE)
        assert_eq!(dimensions(&b), Some((512, 100)));
    }

    #[test]
    fn webp_vp8l_dimensions_parse() {
        // RIFF | size | WEBP | VP8L | signature | packed dims (zeros => 1x1)
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&[0, 0, 0, 0]);
        b.extend_from_slice(b"WEBP");
        b.extend_from_slice(b"VP8L");
        b.extend_from_slice(&[0x2F, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(dimensions(&b), Some((1, 1)));
    }

    #[test]
    fn unknown_format_is_none() {
        assert_eq!(dimensions(b"not an image at all"), None);
        assert_eq!(dimensions(&[]), None);
    }

    // ── TIFF / EXIF helpers ──────────────────────────────────────────────

    /// Build a minimal little-endian TIFF: header + one IFD with `entries`
    /// (tag, type, count, inline value) and room for extra data at `extra_off`.
    fn build_le_tiff(
        entries: &[(u16, u16, u32, u32)],
        extra: &[(usize, Vec<u8>)],
        total: usize,
    ) -> Vec<u8> {
        let mut b = vec![0u8; total];
        b[0..2].copy_from_slice(b"II");
        b[2..4].copy_from_slice(&0x2Au16.to_le_bytes());
        b[4..8].copy_from_slice(&8u32.to_le_bytes()); // IFD0 at 8
        b[8..10].copy_from_slice(&(entries.len() as u16).to_le_bytes());
        for (i, (tag, typ, count, value)) in entries.iter().enumerate() {
            let e = 10 + i * 12;
            b[e..e + 2].copy_from_slice(&tag.to_le_bytes());
            b[e + 2..e + 4].copy_from_slice(&typ.to_le_bytes());
            b[e + 4..e + 8].copy_from_slice(&count.to_le_bytes());
            b[e + 8..e + 12].copy_from_slice(&value.to_le_bytes());
        }
        let next = 10 + entries.len() * 12;
        b[next..next + 4].copy_from_slice(&0u32.to_le_bytes());
        for (off, data) in extra {
            b[*off..*off + data.len()].copy_from_slice(data);
        }
        b
    }

    #[test]
    fn tiff_dimensions_parse() {
        // IFD0 at 8: width (0x0100, SHORT=3) = 256, height (0x0101, SHORT=3) = 64.
        let b = build_le_tiff(
            &[(0x0100, 3, 1, 256), (0x0101, 3, 1, 64)],
            &[],
            10 + 2 * 12 + 4,
        );
        assert_eq!(dimensions(&b), Some((256, 64)));
    }

    #[test]
    fn tiff_big_endian_dimensions_parse() {
        let mut b = vec![0u8; 58];
        b[0..2].copy_from_slice(b"MM");
        b[2..4].copy_from_slice(&42u16.to_be_bytes());
        b[4..8].copy_from_slice(&8u32.to_be_bytes());
        b[8..10].copy_from_slice(&2u16.to_be_bytes());
        // width LONG (type 4) = 512, height LONG = 64, big-endian entries.
        b[10..12].copy_from_slice(&0x0100u16.to_be_bytes());
        b[12..14].copy_from_slice(&4u16.to_be_bytes());
        b[14..18].copy_from_slice(&1u32.to_be_bytes());
        b[18..22].copy_from_slice(&512u32.to_be_bytes());
        b[22..24].copy_from_slice(&0x0101u16.to_be_bytes());
        b[24..26].copy_from_slice(&4u16.to_be_bytes());
        b[26..30].copy_from_slice(&1u32.to_be_bytes());
        b[30..34].copy_from_slice(&64u32.to_be_bytes());
        b[34..38].copy_from_slice(&0u32.to_be_bytes());
        assert_eq!(dimensions(&b), Some((512, 64)));
    }

    #[test]
    fn exif_gps_from_jpeg_roundtrip() {
        // GPS IFD at offset 46: N, 37°0'0", W, 122°0'0".
        let mut gps_ifd = Vec::new();
        gps_ifd.extend_from_slice(&4u16.to_le_bytes()); // 4 entries
        let lat_off: usize = 46 + 2 + 4 * 12 + 4;
        let lon_off: usize = lat_off + 24; // 3 RATIONals = 24 bytes
        let mut e = |tag: u16, typ: u16, count: u32, value: u32, buf: &mut Vec<u8>| {
            buf.extend_from_slice(&tag.to_le_bytes());
            buf.extend_from_slice(&typ.to_le_bytes());
            buf.extend_from_slice(&count.to_le_bytes());
            buf.extend_from_slice(&value.to_le_bytes());
        };
        // GPSLatitudeRef "N\0" (ASCII type 2, count 2, inline)
        let n_ref = [b'N', 0, 0, 0];
        e(0x0001, 2, 2, u32::from_le_bytes(n_ref), &mut gps_ifd);
        // GPSLatitude RATIONAL×3 at lat_off
        e(0x0002, 5, 3, lat_off as u32, &mut gps_ifd);
        // GPSLongitudeRef "W\0"
        let w_ref = [b'W', 0, 0, 0];
        e(0x0003, 2, 2, u32::from_le_bytes(w_ref), &mut gps_ifd);
        // GPSLongitude RATIONAL×3 at lon_off
        e(0x0004, 5, 3, lon_off as u32, &mut gps_ifd);
        gps_ifd.extend_from_slice(&0u32.to_le_bytes()); // next IFD

        let mut rationals = Vec::new();
        for (n, d) in [(37u32, 1u32), (0, 1), (0, 1)] {
            rationals.extend_from_slice(&n.to_le_bytes());
            rationals.extend_from_slice(&d.to_le_bytes());
        }
        let mut lon_rationals = Vec::new();
        for (n, d) in [(122u32, 1u32), (0, 1), (0, 1)] {
            lon_rationals.extend_from_slice(&n.to_le_bytes());
            lon_rationals.extend_from_slice(&d.to_le_bytes());
        }

        // IFD0 with a single GPS-info pointer (0x8825, LONG) → 46.
        let tiff = build_le_tiff(
            &[(0x8825, 4, 1, 46)],
            &[
                (46, gps_ifd),
                (lat_off, rationals),
                (lon_off, lon_rationals),
            ],
            256,
        );

        // Wrap in a JPEG APP1 segment: SOI + APP1(len) + "Exif\0\0" + tiff + EOI.
        let mut jpeg = Vec::new();
        jpeg.extend_from_slice(&[0xFF, 0xD8]);
        jpeg.extend_from_slice(&[0xFF, 0xE1]);
        let seg_len = (2 + 6 + tiff.len()) as u16;
        jpeg.extend_from_slice(&seg_len.to_be_bytes());
        jpeg.extend_from_slice(b"Exif\0\0");
        jpeg.extend_from_slice(&tiff);
        jpeg.extend_from_slice(&[0xFF, 0xD9]);

        assert_eq!(exif_gps(&jpeg), Some((37.0, -122.0)));
        // A bare TIFF payload works too.
        assert_eq!(exif_gps(&tiff), Some((37.0, -122.0)));
    }

    #[test]
    fn exif_gps_absent_is_none() {
        assert_eq!(exif_gps(b"nope"), None);
        // TIFF without GPS IFD.
        let b = build_le_tiff(&[(0x0100, 3, 1, 5)], &[], 40);
        assert_eq!(exif_gps(&b), None);
    }
}
