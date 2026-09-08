//! `fileInfo` query — mirrors `plain-app` `web/schemas/FileQueryGraphQL.kt`.
//!
//! Used by the chat lightbox's right-side info panel. In Tauri popup
//! windows the lightbox renders without a device session, so the request
//! lands on the local server. Without this resolver the popup lightbox
//! fails every query with `Unknown field "fileInfo"` and the right-side
//! info button is broken.
//!
//! Scope is intentionally narrower than the Kotlin original:
//! - `path` is resolved through `resolve_uri` (handles `fid:`, `app://`,
//!   relative, and absolute forms), so we can read metadata for files
//!   already in the local content-addressable store.
//! - Image dimensions and EXIF GPS come from plain-rs (hand-parsed from the JPEG /
//!   PNG / GIF / WebP / BMP / TIFF headers. We only need width / height
//!   and GPS coordinates, so a hand-rolled EXIF reader keeps us off a
//!   third-party dependency.
//! - Video / audio metadata (width, height, duration) require
//!   `MediaMetadataRetriever` equivalents we don't ship. The desktop
//!   main window still routes through the device server for those
//!   fields; in local mode they read as `0` and the popup's right
//!   panel — collapsed by default — is unaffected.
//! - `tags` is always empty in local mode; plain-web doesn't yet
//!   persist tag relations.

use std::path::Path;

use async_graphql::{Context, Object, Result as GqlResult};

use crate::local::graphql::context::AppCtx;
use crate::local::graphql::schema::types::{AudioFileInfo, FileInfo, ImageFileInfo, Location, MediaFileInfo, Tag, VideoFileInfo};
use crate::local::server::uri::resolve_uri;

#[derive(Default)]
pub struct FileInfoQuery;

#[Object]
impl FileInfoQuery {
    /// Mirrors `plain-app` `query("fileInfo") { id path fileName }` —
    /// `id` and `fileName` are accepted for schema parity but only `path`
    /// is used to locate the file.
    async fn file_info(
        &self,
        ctx: &Context<'_>,
        id: String,
        path: String,
        file_name: String,
    ) -> GqlResult<FileInfo> {
        let _ = (id, file_name);
        let c = ctx.data_unchecked::<std::sync::Arc<AppCtx>>();

        let real = resolve_uri(&path, &c.data_dir);
        let (updated_at, size) = read_file_meta(&real);
        let data = classify_and_load(&real);

        Ok(FileInfo {
            path,
            updated_at,
            size,
            tags: Vec::<Tag>::new(),
            data,
        })
    }
}

fn read_file_meta(path: &Path) -> (String, i64) {
    let Ok(meta) = std::fs::metadata(path) else {
        return (String::new(), 0);
    };
    let updated_at = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| unix_secs_to_iso8601(d.as_secs() as i64))
        .unwrap_or_default();
    (updated_at, meta.len() as i64)
}

/// Format a unix-second timestamp as `YYYY-MM-DDTHH:MM:SSZ` without
/// pulling in `chrono`. Local mode timestamps are best-effort (UTC).
fn unix_secs_to_iso8601(secs: i64) -> String {
    if secs <= 0 {
        return String::new();
    }
    let days = secs.div_euclid(86_400);
    let mut year: i64 = 1970;
    let mut remaining = days;
    loop {
        let leap = is_leap(year);
        let dy = if leap { 366 } else { 365 };
        if remaining < dy {
            break;
        }
        remaining -= dy;
        year += 1;
    }
    let leap = is_leap(year);
    let md: [i64; 12] = if leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month: i64 = 1;
    for &dm in md.iter() {
        if remaining < dm {
            break;
        }
        remaining -= dm;
        month += 1;
    }
    let day = remaining + 1;
    let tod = secs.rem_euclid(86_400);
    let hh = tod / 3600;
    let mm = (tod % 3600) / 60;
    let ss = tod % 60;
    format!("{year:04}-{month:02}-{day:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

fn is_leap(y: i64) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

fn classify_and_load(path: &Path) -> Option<MediaFileInfo> {
    if !path.is_file() {
        return None;
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)?;
    if is_image_ext(&ext) {
        load_image(path, &ext).map(MediaFileInfo::Image)
    } else if is_video_ext(&ext) {
        // No video decoder in local mode — return zeros so the union
        // variant is still selectable on the client.
        Some(MediaFileInfo::Video(VideoFileInfo {
            width: 0,
            height: 0,
            duration: 0,
            location: None,
        }))
    } else if is_audio_ext(&ext) {
        Some(MediaFileInfo::Audio(AudioFileInfo {
            duration: 0,
            location: None,
        }))
    } else {
        None
    }
}

fn is_image_ext(ext: &str) -> bool {
    matches!(
        ext,
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tif" | "tiff"
    )
}

fn is_video_ext(ext: &str) -> bool {
    matches!(
        ext,
        "mp4" | "m4v" | "mov" | "mkv" | "webm" | "avi" | "3gp"
    )
}

fn is_audio_ext(ext: &str) -> bool {
    matches!(ext, "mp3" | "m4a" | "aac" | "wav" | "flac" | "ogg" | "opus")
}

// ── Image dimensions + EXIF GPS ──────────────────────────────────────────────
//
// Both come from the shared plain_rs::utils::image_dimensions module
// (JPEG / PNG / GIF / BMP / WebP / ICO / TIFF dimensions, and EXIF GPS
// for JPEG + TIFF payloads).

fn load_image(path: &Path, ext: &str) -> Option<ImageFileInfo> {
    let bytes = std::fs::read(path).ok()?;
    let (width, height) = plain_rs::utils::image_dimensions::dimensions(&bytes)?;
    let location = if matches!(ext, "jpg" | "jpeg" | "tif" | "tiff") {
        plain_rs::utils::image_dimensions::exif_gps(&bytes)
            .map(|(latitude, longitude)| Location { latitude, longitude })
    } else {
        None
    };
    Some(ImageFileInfo {
        width,
        height,
        location,
    })
}

#[cfg(test)]
mod tests {
    use super::*;



    #[test]
    fn leap_year() {
        assert!(is_leap(2000));
        assert!(is_leap(2024));
        assert!(!is_leap(2023));
        assert!(!is_leap(1900));
    }

}
