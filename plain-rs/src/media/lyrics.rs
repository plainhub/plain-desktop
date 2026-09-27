//! Embedded-lyrics extraction — 1:1 port of plain-app `EmbeddedLyrics.kt`.
//!
//! Reads the source strictly forward. Supported containers: ID3v2 USLT/ULT
//! (mp3), Vorbis comments `LYRICS`/`UNSYNCEDLYRICS` (flac) and the MP4 `©lyr`
//! atom (m4a). Anything else yields an empty string.

use std::io::Read;

const MAX_TAG_BYTES: usize = 8 * 1024 * 1024;
const MAX_MOOV_BYTES: usize = 16 * 1024 * 1024;

/// Extract the embedded lyrics from an audio file.
pub fn extract_lyrics_from_path(path: &str) -> String {
    match std::fs::File::open(path) {
        Ok(f) => {
            let mut reader = std::io::BufReader::new(f);
            extract(&mut reader)
        }
        Err(_) => String::new(),
    }
}

fn read_fully(reader: &mut impl Read, length: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(length.min(MAX_MOOV_BYTES.max(MAX_TAG_BYTES)));
    let mut take = reader.take(length as u64);
    let _ = take.read_to_end(&mut out);
    out
}

fn skip_fully(reader: &mut impl Read, length: u64) {
    let mut remaining = length;
    let mut scratch = [0u8; 8192];
    while remaining > 0 {
        let want = remaining.min(scratch.len() as u64) as usize;
        match reader.read(&mut scratch[..want]) {
            Ok(0) | Err(_) => return,
            Ok(n) => remaining -= n as u64,
        }
    }
}

pub fn extract(reader: &mut impl Read) -> String {
    let mut head = [0u8; 8];
    let n = read_up_to(reader, &mut head);
    let head = &head[..n];
    if head.len() >= 3 && &head[0..3] == b"ID3" {
        extract_id3(reader, head)
    } else if head.len() >= 8 && &head[4..8] == b"ftyp" {
        extract_mp4(reader, head)
    } else if head.len() >= 8 && &head[0..4] == b"fLaC" {
        extract_flac(reader, head)
    } else {
        String::new()
    }
}

/// Partial read (short reads allowed), like `InputStream.read`.
fn read_up_to(reader: &mut impl Read, buf: &mut [u8]) -> usize {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) | Err(_) => break,
            Ok(n) => filled += n,
        }
    }
    filled
}

fn extract_id3(reader: &mut impl Read, head: &[u8]) -> String {
    let mut header = head.to_vec();
    if header.len() < 10 {
        let mut rest = vec![0u8; 10 - header.len()];
        let n = read_up_to(reader, &mut rest);
        header.extend_from_slice(&rest[..n]);
    }
    if header.len() < 10 {
        return String::new();
    }
    let version = header[3] as u32;
    let flags = header[5] as u32;
    let tag_size = syncsafe(&header, 6).min(MAX_TAG_BYTES as u32) as usize;
    let mut data = read_fully(reader, tag_size);
    if flags & 0x80 != 0 {
        data = remove_unsync(&data);
    }

    let mut pos = 0usize;
    if flags & 0x40 != 0 && data.len() >= 4 {
        // Extended header: v2.4 sizes include themselves, v2.3 do not.
        pos += if version >= 4 {
            syncsafe(&data, 0) as usize
        } else {
            be32(&data, 0) as usize + 4
        };
    }
    let id_size = if version >= 3 { 4 } else { 3 };
    let header_size = id_size + if version >= 3 { 6 } else { 3 };
    while pos + header_size <= data.len() {
        let id = latin1(&data[pos..pos + id_size]);
        if id.is_empty() || id.starts_with('\u{0000}') {
            break; // padding
        }
        let size = if version >= 4 {
            syncsafe(&data, pos + id_size) as usize
        } else if version == 3 {
            be32(&data, pos + id_size) as usize
        } else {
            be24(&data, pos + id_size) as usize
        };
        let data_start = pos + header_size;
        if size == 0 || data_start + size > data.len() {
            break;
        }
        if id == "USLT" || id == "ULT" {
            let frame = &data[data_start..data_start + size];
            let frame_unsync = version >= 4 && data[pos + 9] & 0x02 != 0;
            let cleaned = if frame_unsync {
                remove_unsync(frame)
            } else {
                frame.to_vec()
            };
            if let Some(text) = uslt_text(&cleaned) {
                return text;
            }
        }
        pos = data_start + size;
    }
    String::new()
}

fn uslt_text(frame: &[u8]) -> Option<String> {
    if frame.len() < 5 {
        return None;
    }
    let encoding = frame[0];
    // Locate the NUL-terminated content descriptor at byte level: one NUL
    // for latin1/utf8, an even-aligned NUL pair for UTF-16. Without one the
    // whole payload is treated as lyrics.
    let utf16 = encoding == 0x01 || encoding == 0x02;
    let mut text_start = 4;
    if utf16 {
        let mut i = 4;
        while i + 1 < frame.len() {
            if frame[i] == 0 && frame[i + 1] == 0 {
                text_start = i + 2;
                break;
            }
            i += 2;
        }
    } else {
        let mut i = 4;
        while i < frame.len() {
            if frame[i] == 0 {
                text_start = i + 1;
                break;
            }
            i += 1;
        }
    }
    let text = match encoding {
        // 0x01: UTF-16 with BOM (BOM decides, default big endian);
        // 0x02: UTF-16BE (forced big endian).
        0x01 => decode_utf16(frame, text_start, None),
        0x02 => decode_utf16(frame, text_start, Some(true)),
        0x03 => String::from_utf8_lossy(&frame[text_start..]).into_owned(),
        _ => latin1(&frame[text_start..]),
    };
    let text = text.trim_start_matches('\u{FEFF}').trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

fn extract_flac(reader: &mut impl Read, head: &[u8]) -> String {
    // The first metadata block header was consumed together with the sniff.
    let mut kind = head[4] & 0x7F;
    let mut last = head[4] & 0x80 != 0;
    let mut size = be24(head, 5) as usize;
    let mut first = true;
    for _ in 0..1024 {
        if !first {
            let mut header = [0u8; 4];
            if read_up_to(reader, &mut header) < 4 {
                return String::new();
            }
            kind = header[0] & 0x7F;
            last = header[0] & 0x80 != 0;
            size = be24(&header, 1) as usize;
        }
        first = false;
        if kind == 4 {
            // VORBIS_COMMENT
            return vorbis_comment_lyrics(&read_fully(reader, size.min(MAX_TAG_BYTES)));
        }
        skip_fully(reader, size as u64);
        if last {
            return String::new();
        }
    }
    String::new()
}

fn vorbis_comment_lyrics(data: &[u8]) -> String {
    let mut pos = 0usize;
    fn le32(data: &[u8], pos: &mut usize) -> Option<u32> {
        if *pos + 4 > data.len() {
            return None;
        }
        let v = u32::from_le_bytes([data[*pos], data[*pos + 1], data[*pos + 2], data[*pos + 3]]);
        *pos += 4;
        Some(v)
    }
    let vendor_len = match le32(data, &mut pos) {
        Some(v) => v as usize,
        None => return String::new(),
    };
    if pos + vendor_len > data.len() {
        return String::new();
    }
    pos += vendor_len;
    let count = match le32(data, &mut pos) {
        Some(v) => v as usize,
        None => return String::new(),
    };
    for _ in 0..count.min(1024) {
        let len = match le32(data, &mut pos) {
            Some(v) => v as usize,
            None => return String::new(),
        };
        if len == 0 || pos + len > data.len() {
            return String::new();
        }
        let comment = String::from_utf8_lossy(&data[pos..pos + len]).into_owned();
        pos += len;
        if let Some(eq) = comment.find('=') {
            let key = &comment[..eq];
            if key.eq_ignore_ascii_case("LYRICS") || key.eq_ignore_ascii_case("UNSYNCEDLYRICS") {
                let value = comment[eq + 1..].trim().to_string();
                if !value.is_empty() {
                    return value;
                }
            }
        }
    }
    String::new()
}

fn extract_mp4(reader: &mut impl Read, head: &[u8]) -> String {
    // The ftyp header is already consumed with the sniff, skip its content.
    let ftyp_size = be32(head, 0) as u64;
    if ftyp_size < 8 {
        return String::new();
    }
    skip_fully(reader, ftyp_size - 8);
    for _ in 0..64 {
        let mut header = [0u8; 8];
        if read_up_to(reader, &mut header) < 8 {
            return String::new();
        }
        let size = be32(&header, 0) as u64;
        let kind = latin1(&header[4..8]);
        // 64-bit extended box sizes are not worth the plumbing here
        if size < 8 {
            return String::new();
        }
        if kind == "moov" {
            let moov = read_fully(reader, (size - 8).min(MAX_MOOV_BYTES as u64) as usize);
            return mp4_moov_lyrics(&moov);
        }
        skip_fully(reader, size - 8);
    }
    String::new()
}

fn mp4_moov_lyrics(moov: &[u8]) -> String {
    let (udta_start, udta_end) = match find_box(moov, 0, moov.len(), b"udta") {
        Some(r) => r,
        None => return String::new(),
    };
    let (meta_start, meta_end) = match find_box(moov, udta_start, udta_end, b"meta") {
        Some(r) => r,
        None => return String::new(),
    };
    // meta carries a 4-byte version/flags field before its children
    let (ilst_start, ilst_end) = match find_box(moov, meta_start + 4, meta_end, b"ilst") {
        Some(r) => r,
        None => return String::new(),
    };
    // ©lyr is a 4-byte ISO-8859-1 box type: 0xA9 + "lyr" (UTF-8 would be
    // five bytes and never match a real file).
    let (lyr_start, lyr_end) = match find_box(moov, ilst_start, ilst_end, b"\xA9lyr") {
        Some(r) => r,
        None => return String::new(),
    };
    let (data_start, data_end) = match find_box(moov, lyr_start, lyr_end, b"data") {
        Some(r) => r,
        None => return String::new(),
    };
    if data_start + 8 > data_end {
        return String::new();
    }
    // data payload: 1-byte type indicator + 3 flags, 4-byte locale, then text
    String::from_utf8_lossy(&moov[data_start + 8..data_end])
        .trim()
        .to_string()
}

/// Finds a child box in `data[from..until]` and returns its content range.
fn find_box(data: &[u8], from: usize, until: usize, kind: &[u8]) -> Option<(usize, usize)> {
    let mut pos = from;
    while pos + 8 <= until {
        let size = be32(data, pos) as u64;
        if size < 8 {
            return None;
        }
        let end = (pos as u64 + size).min(until as u64) as usize;
        if &data[pos + 4..pos + 8] == kind {
            return Some((pos + 8, end));
        }
        pos = end;
    }
    None
}

fn latin1(data: &[u8]) -> String {
    data.iter().map(|&b| b as char).collect()
}

/// `force`: `None` means a leading BOM decides (defaulting to big endian);
/// `Some(big_endian)` forces the byte order.
fn decode_utf16(data: &[u8], start: usize, force: Option<bool>) -> String {
    let mut from = start;
    let mut big = force.unwrap_or(true);
    if force.is_none() && data.len().saturating_sub(start) >= 2 {
        if data[start] == 0xFF && data[start + 1] == 0xFE {
            big = false;
            from += 2;
        } else if data[start] == 0xFE && data[start + 1] == 0xFF {
            from += 2;
        }
    }
    let mut units = Vec::with_capacity((data.len() - from) / 2);
    let mut i = from;
    while i + 1 < data.len() {
        let b0 = data[i] as u16;
        let b1 = data[i + 1] as u16;
        let unit = if big { (b0 << 8) | b1 } else { (b1 << 8) | b0 };
        i += 2;
        units.push(unit);
    }
    String::from_utf16_lossy(&units)
}

fn be24(data: &[u8], offset: usize) -> u32 {
    ((data[offset] as u32) << 16) | ((data[offset + 1] as u32) << 8) | data[offset + 2] as u32
}

fn be32(data: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

fn syncsafe(data: &[u8], offset: usize) -> u32 {
    ((data[offset] as u32 & 0x7F) << 21)
        | ((data[offset + 1] as u32 & 0x7F) << 14)
        | ((data[offset + 2] as u32 & 0x7F) << 7)
        | (data[offset + 3] as u32 & 0x7F)
}

fn remove_unsync(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        out.push(data[i]);
        i += if data[i] == 0xFF && i + 1 < data.len() && data[i + 1] == 0x00 {
            2
        } else {
            1
        };
    }
    out
}

#[cfg(test)]
#[path = "../../tests/unit/media/lyrics.rs"]
mod tests;
