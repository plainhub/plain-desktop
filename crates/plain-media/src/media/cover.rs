//! Cover art extraction for audio/video thumbnails.
//!
//! 1:1 port of Go's `cover.go`, `cover_mp3.go`, `cover_flac.go`,
//! `cover_mp4.go`. Uses `lofty` for embedded cover extraction.
//!
//! Priority: sidecar file > embedded cover.

use lofty::file::TaggedFileExt;
use lofty::picture::{Picture, PictureType};
use lofty::probe::Probe;
use lofty::tag::TagType;
use std::path::Path;

/// Result of cover extraction: raw image bytes + MIME type.
pub struct CoverData {
    pub bytes: Vec<u8>,
    pub mime: String,
}

/// Attempt to extract a cover image suitable for thumbnailing.
/// Returns `None` if no cover is found.
///
/// Priority: sidecar (cover.jpg, folder.jpg, etc.) > embedded (ID3 APIC,
/// FLAC PICTURE, MP4 covr).
pub fn extract_cover(path: &str) -> Option<CoverData> {
    // 1. Sidecar lookup.
    if let Some(p) = find_sidecar_cover_path(path) {
        if let Ok(bytes) = std::fs::read(&p) {
            if !bytes.is_empty() {
                let mime = guess_mime_from_ext(&p);
                return Some(CoverData { bytes, mime });
            }
        }
    }

    // 2. Embedded cover extraction.
    extract_embedded_cover(path)
}

/// Return the file path whose metadata should be used to invalidate
/// thumbnail caches for `path`.
pub fn thumbnail_cache_ref_path(path: &str) -> String {
    find_sidecar_cover_path(path).unwrap_or_else(|| path.to_string())
}

// ---------------------------------------------------------------------------
// Sidecar lookup
// ---------------------------------------------------------------------------

fn find_sidecar_cover_path(path: &str) -> Option<String> {
    let p = Path::new(path);
    let dir = p.parent()?;
    let stem = p.file_stem()?.to_str()?;

    let candidates = [
        format!("{stem}.jpg"),
        format!("{stem}.jpeg"),
        format!("{stem}.webp"),
        format!("{stem}.png"),
        format!("{stem}.gif"),
        "cover.jpg".into(),
        "cover.jpeg".into(),
        "cover.webp".into(),
        "cover.png".into(),
        "cover.gif".into(),
        "folder.jpg".into(),
        "folder.jpeg".into(),
        "folder.webp".into(),
        "folder.png".into(),
        "folder.gif".into(),
    ];

    for name in &candidates {
        let full = dir.join(name);
        if let Ok(meta) = std::fs::metadata(&full) {
            if meta.is_file() && meta.len() > 0 {
                return Some(full.to_string_lossy().to_string());
            }
        }
    }
    None
}

fn guess_mime_from_ext(path: &str) -> String {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg".into(),
        "webp" => "image/webp".into(),
        "png" => "image/png".into(),
        "gif" => "image/gif".into(),
        _ => "image/jpeg".into(),
    }
}

// ---------------------------------------------------------------------------
// Embedded cover extraction via lofty
// ---------------------------------------------------------------------------

fn extract_embedded_cover(path: &str) -> Option<CoverData> {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "mp3" | "flac" | "ogg" | "opus" | "m4a" | "mp4" | "m4v" | "mov" | "3gp" | "3gpp"
        | "wma" | "wav" | "aac" => {}
        _ => return None,
    }

    let tagged_file = Probe::open(path).ok()?.read().ok()?;

    // Try each tag type in priority order.
    for tag_type in [
        TagType::Id3v2,
        TagType::VorbisComments,
        TagType::Mp4Ilst,
        TagType::Ape,
    ] {
        if let Some(tag) = tagged_file.tag(tag_type) {
            // Look for front cover first.
            if let Some(pic) = tag.get_picture_type(PictureType::CoverFront) {
                return picture_to_cover_data(pic);
            }
            // Try "Other" picture type.
            if let Some(pic) = tag.get_picture_type(PictureType::Other) {
                return picture_to_cover_data(pic);
            }
            // Last resort: any picture in the tag.
            let pics = tag.pictures();
            if let Some(pic) = pics.first() {
                return picture_to_cover_data(pic);
            }
        }
    }

    // Fallback: primary tag.
    if let Some(tag) = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag())
    {
        let pics = tag.pictures();
        if let Some(pic) = pics.first() {
            return picture_to_cover_data(pic);
        }
    }

    None
}

fn picture_to_cover_data(pic: &Picture) -> Option<CoverData> {
    let data = pic.data();
    if data.is_empty() || data.len() > 25 * 1024 * 1024 {
        return None;
    }
    let mime = pic
        .mime_type()
        .map(|m| format!("{m}"))
        .unwrap_or_else(|| "image/jpeg".to_string());
    Some(CoverData {
        bytes: data.to_vec(),
        mime,
    })
}

#[cfg(test)]
#[path = "../../tests/unit/media/cover.rs"]
mod tests;
