//! Unit tests for `src/media/lyrics.rs` — 1:1 mirror of plain-app
//! `EmbeddedLyricsTest.kt` (same hand-built fixtures, same expectations).
use super::*;

fn extract_bytes(data: &[u8]) -> String {
    let mut cursor = std::io::Cursor::new(data.to_vec());
    extract(&mut cursor)
}

fn be32(size: usize) -> Vec<u8> {
    vec![
        (size >> 24) as u8,
        (size >> 16) as u8,
        (size >> 8) as u8,
        size as u8,
    ]
}

fn be24(size: usize) -> Vec<u8> {
    vec![(size >> 16) as u8, (size >> 8) as u8, size as u8]
}

fn le32(size: usize) -> Vec<u8> {
    vec![
        size as u8,
        (size >> 8) as u8,
        (size >> 16) as u8,
        (size >> 24) as u8,
    ]
}

fn syncsafe(size: usize) -> Vec<u8> {
    vec![
        ((size >> 21) & 0x7F) as u8,
        ((size >> 14) & 0x7F) as u8,
        ((size >> 7) & 0x7F) as u8,
        (size & 0x7F) as u8,
    ]
}

/// ISO-8859-1 so box types like ©lyr stay single-byte, matching real files.
fn box_of(kind: &str, content: &[u8]) -> Vec<u8> {
    box_bytes(kind.as_bytes(), content)
}

/// Byte-typed box: the ©lyr/©nam atoms carry a single-byte 0xA9, which is
/// ISO-8859-1 in real files (UTF-8 would be two bytes).
fn box_bytes(kind: &[u8], content: &[u8]) -> Vec<u8> {
    let mut out = be32(content.len() + 8);
    out.extend_from_slice(kind);
    out.extend_from_slice(content);
    out
}

#[test]
fn id3v23_utf8_uslt() {
    let lyrics = "[00:01.00]Hello\n[00:05.00]World";
    let mut frame_body = vec![3];
    frame_body.extend_from_slice(b"eng");
    frame_body.push(0);
    frame_body.extend_from_slice(lyrics.as_bytes());
    let mut frame = b"USLT".to_vec();
    frame.extend_from_slice(&be32(frame_body.len()));
    frame.extend_from_slice(&[0, 0]);
    frame.extend_from_slice(&frame_body);
    let mut header = b"ID3".to_vec();
    header.extend_from_slice(&[3, 0, 0]);
    header.extend_from_slice(&syncsafe(frame.len()));
    header.extend_from_slice(&frame);
    assert_eq!(extract_bytes(&header), lyrics);
}

#[test]
fn id3v24_utf16_uslt() {
    let lyrics = "同步歌词";
    let encoded: Vec<u8> = lyrics
        .encode_utf16()
        .flat_map(|u| u.to_be_bytes().to_vec())
        .collect();
    let bom_encoded = {
        let mut v = vec![0xFE, 0xFF];
        v.extend_from_slice(&encoded);
        v
    };
    let mut frame_body = vec![1];
    frame_body.extend_from_slice(b"chi");
    frame_body.extend_from_slice(&[0, 0]);
    frame_body.extend_from_slice(&bom_encoded);
    let mut frame = b"USLT".to_vec();
    frame.extend_from_slice(&syncsafe(frame_body.len()));
    frame.extend_from_slice(&[0, 0]);
    frame.extend_from_slice(&frame_body);
    let mut header = b"ID3".to_vec();
    header.extend_from_slice(&[4, 0, 0]);
    header.extend_from_slice(&syncsafe(frame.len()));
    header.extend_from_slice(&frame);
    assert_eq!(extract_bytes(&header), lyrics);
}

#[test]
fn id3_uslt_skips_descriptor() {
    let lyrics = "plain text lyrics";
    let mut frame_body = vec![0];
    frame_body.extend_from_slice(b"eng");
    frame_body.extend_from_slice(b"desc");
    frame_body.push(0);
    frame_body.extend_from_slice(lyrics.as_bytes());
    let mut frame = b"USLT".to_vec();
    frame.extend_from_slice(&be32(frame_body.len()));
    frame.extend_from_slice(&[0, 0]);
    frame.extend_from_slice(&frame_body);
    let mut header = b"ID3".to_vec();
    header.extend_from_slice(&[3, 0, 0]);
    header.extend_from_slice(&syncsafe(frame.len()));
    header.extend_from_slice(&frame);
    assert_eq!(extract_bytes(&header), lyrics);
}

#[test]
fn id3_without_lyrics_returns_empty() {
    let mut frame_body = vec![0];
    frame_body.extend_from_slice(b"title");
    frame_body.push(0);
    let mut frame = b"TIT2".to_vec();
    frame.extend_from_slice(&be32(frame_body.len()));
    frame.extend_from_slice(&[0, 0]);
    frame.extend_from_slice(&frame_body);
    let mut header = b"ID3".to_vec();
    header.extend_from_slice(&[3, 0, 0]);
    header.extend_from_slice(&syncsafe(frame.len()));
    header.extend_from_slice(&frame);
    assert_eq!(extract_bytes(&header), "");
}

#[test]
fn flac_vorbis_comment_lyrics() {
    let lyrics = "[00:10.00] lyrics line";
    let comment = format!("LYRICS={lyrics}").into_bytes();
    let mut block = le32(3);
    block.extend_from_slice(b"ref");
    block.extend_from_slice(&le32(1));
    block.extend_from_slice(&le32(comment.len()));
    block.extend_from_slice(&comment);
    let mut data = b"fLaC".to_vec();
    data.extend_from_slice(&[0]);
    data.extend_from_slice(&be24(18));
    data.extend_from_slice(&[0u8; 18]); // STREAMINFO, not last
    data.extend_from_slice(&[0x84]);
    data.extend_from_slice(&be24(block.len()));
    data.extend_from_slice(&block); // VORBIS_COMMENT, last
    assert_eq!(extract_bytes(&data), lyrics);
}

#[test]
fn flac_unsyncedlyrics_key_accepted() {
    let lyrics = "flac lyrics";
    let comment = format!("UNSYNCEDLYRICS={lyrics}").into_bytes();
    let mut block = le32(0);
    block.extend_from_slice(&le32(1));
    block.extend_from_slice(&le32(comment.len()));
    block.extend_from_slice(&comment);
    let mut data = b"fLaC".to_vec();
    data.extend_from_slice(&[0x84]);
    data.extend_from_slice(&be24(block.len()));
    data.extend_from_slice(&block);
    assert_eq!(extract_bytes(&data), lyrics);
}

#[test]
fn mp4_copyright_lyr_atom() {
    let lyrics = "m4a embedded lyrics";
    let mut data_payload = vec![0, 0, 0, 1, 0, 0, 0, 0];
    data_payload.extend_from_slice(lyrics.as_bytes());
    let lyr_data = box_of("data", &data_payload);
    let ilst = box_of("ilst", &box_bytes(b"\xA9lyr", &lyr_data));
    let mut meta_content = vec![0, 0, 0, 0];
    meta_content.extend_from_slice(&ilst);
    let meta = box_of("meta", &meta_content);
    let udta = box_of("udta", &meta);
    let moov = box_of("moov", &udta);
    let mut ftyp_content = b"M4A ".to_vec();
    ftyp_content.extend_from_slice(&[0, 0, 0, 0]);
    let ftyp = box_of("ftyp", &ftyp_content);
    let mut file = ftyp;
    file.extend_from_slice(&moov);
    assert_eq!(extract_bytes(&file), lyrics);
}

#[test]
fn mp4_without_lyrics_returns_empty() {
    let mut nam_payload = vec![0, 0, 0, 1, 0, 0, 0, 0];
    nam_payload.extend_from_slice(b"song");
    let nam_data = box_of("data", &nam_payload);
    let ilst = box_of("ilst", &box_bytes(b"\xA9nam", &nam_data));
    let mut meta_content = vec![0, 0, 0, 0];
    meta_content.extend_from_slice(&ilst);
    let meta = box_of("meta", &meta_content);
    let udta = box_of("udta", &meta);
    let moov = box_of("moov", &udta);
    let mut ftyp_content = b"M4A ".to_vec();
    ftyp_content.extend_from_slice(&[0, 0, 0, 0]);
    let ftyp = box_of("ftyp", &ftyp_content);
    let mut file = ftyp;
    file.extend_from_slice(&moov);
    assert_eq!(extract_bytes(&file), "");
}

#[test]
fn unknown_container_returns_empty() {
    assert_eq!(extract_bytes(b"RIFFxxxxWAVE"), "");
    assert_eq!(extract_bytes(b"OggSxxxx"), "");
    assert_eq!(extract_bytes(b""), "");
}

#[test]
fn file_path_extraction_roundtrips() {
    // A real temp file with an ID3v2.3 USLT frame.
    let mut header = b"ID3".to_vec();
    let mut frame_body = vec![0];
    frame_body.extend_from_slice(b"eng");
    frame_body.push(0);
    frame_body.extend_from_slice(b"file lyrics");
    let mut frame = b"USLT".to_vec();
    frame.extend_from_slice(&be32(frame_body.len()));
    frame.extend_from_slice(&[0, 0]);
    frame.extend_from_slice(&frame_body);
    header.extend_from_slice(&[3, 0, 0]);
    header.extend_from_slice(&syncsafe(frame.len()));
    header.extend_from_slice(&frame);

    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("song.mp3");
    std::fs::write(&p, &header).unwrap();
    assert_eq!(extract_lyrics_from_path(p.to_str().unwrap()), "file lyrics");
    // Missing file → empty, no error.
    assert_eq!(extract_lyrics_from_path("/nonexistent/x.mp3"), "");
}
