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

use async_graphql::{Context, ID, Object, Result as GqlResult};

use super::media::types::Long;
use super::types::Mount;
use crate::api::context::AppCtx;
use crate::http_server::main_schemas::types::{
    AudioFileInfo, FileInfo, ImageFileInfo, Location, MediaFileInfo, VideoFileInfo,
};
use crate::server::uri::resolve_uri;

#[derive(Default)]
pub struct FileInfoQuery;

#[Object]
impl FileInfoQuery {
    async fn mounts(&self) -> Vec<Mount> {
        let disks = sysinfo::Disks::new_with_refreshed_list();
        disks
            .iter()
            .filter(|disk| should_include_mount(disk.mount_point().to_string_lossy().as_ref()))
            .map(|disk| {
                let mount_point = disk.mount_point().to_string_lossy().into_owned();
                let total_bytes = disk.total_space().min(i64::MAX as u64) as i64;
                let free_bytes = disk.available_space().min(i64::MAX as u64) as i64;
                let fs_type = disk.file_system().to_string_lossy().into_owned();
                let remote = is_remote_filesystem(&fs_type);
                Mount {
                    id: ID(mount_point.clone()),
                    name: disk.name().to_string_lossy().into_owned(),
                    path: mount_point.clone(),
                    mount_point: mount_point.clone(),
                    fs_type,
                    total_bytes: Long(total_bytes),
                    used_bytes: Long(total_bytes.saturating_sub(free_bytes)),
                    free_bytes: Long(free_bytes),
                    remote,
                    alias: String::new(),
                    drive_type: if mount_point.starts_with("/Volumes/") {
                        crate::api::enums::DriveType::UsbStorage
                    } else {
                        crate::api::enums::DriveType::InternalStorage
                    },
                    disk_id: mount_point,
                }
            })
            .collect()
    }

    /// Mirrors the plain-app contract `fileInfo(path, fileName)` — `path`
    /// locates the file; `fileName` is an optional display hint that selects
    /// the media-info probe on the phone and is unused here.
    async fn file_info(
        &self,
        ctx: &Context<'_>,
        path: String,
        file_name: Option<String>,
    ) -> GqlResult<FileInfo> {
        let _ = file_name;
        let c = ctx.data_unchecked::<std::sync::Arc<AppCtx>>();

        let real = resolve_uri(&path, &c.data_dir);
        let (updated_at, size) = read_file_meta(&real)?;
        let data = classify_and_load(&real);

        Ok(FileInfo {
            path,
            updated_at,
            size: Long(size),
            data,
        })
    }
}

fn should_include_mount(mount_point: &str) -> bool {
    #[cfg(target_os = "macos")]
    if mount_point == "/System/Volumes/Data" {
        return false;
    }
    let _ = mount_point;
    true
}

fn is_remote_filesystem(fs_type: &str) -> bool {
    let fs_type = fs_type.to_ascii_lowercase();
    ["nfs", "smb", "cifs", "sshfs", "webdav"]
        .iter()
        .any(|kind| fs_type.contains(kind))
}

fn read_file_meta(path: &Path) -> GqlResult<(String, i64)> {
    let meta = std::fs::metadata(path)
        .map_err(|e| async_graphql::Error::new(format!("file metadata unavailable: {e}")))?;
    let updated_at = meta
        .modified()
        .and_then(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .map_err(std::io::Error::other)
        })
        .map(|d| unix_secs_to_iso8601(d.as_secs() as i64))
        .map_err(|e| {
            async_graphql::Error::new(format!("file modification time unavailable: {e}"))
        })?;
    let size = i64::try_from(meta.len())
        .map_err(|e| async_graphql::Error::new(format!("file size exceeds Long range: {e}")))?;
    Ok((updated_at, size))
}

/// Format a unix-second timestamp as `YYYY-MM-DDTHH:MM:SSZ` without
/// pulling in `chrono`. Local mode timestamps are best-effort (UTC).
fn unix_secs_to_iso8601(secs: i64) -> String {
    if secs < 0 {
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
            duration_ms: Long(0),
            location: None,
        }))
    } else if is_audio_ext(&ext) {
        Some(MediaFileInfo::Audio(AudioFileInfo {
            duration_ms: Long(0),
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
    matches!(ext, "mp4" | "m4v" | "mov" | "mkv" | "webm" | "avi" | "3gp")
}

fn is_audio_ext(ext: &str) -> bool {
    matches!(ext, "mp3" | "m4a" | "aac" | "wav" | "flac" | "ogg" | "opus")
}

// ── Image dimensions + EXIF GPS ──────────────────────────────────────────────
//
// Both come from the shared crate::utils::image_dimensions module
// (JPEG / PNG / GIF / BMP / WebP / ICO / TIFF dimensions, and EXIF GPS
// for JPEG + TIFF payloads).

fn load_image(path: &Path, ext: &str) -> Option<ImageFileInfo> {
    let bytes = std::fs::read(path).ok()?;
    let (width, height) = crate::utils::image_dimensions::dimensions(&bytes)?;
    let location = if matches!(ext, "jpg" | "jpeg" | "tif" | "tiff") {
        crate::utils::image_dimensions::exif_gps(&bytes).map(|(latitude, longitude)| Location {
            latitude,
            longitude,
        })
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

#[cfg(test)]
#[path = "../../../tests/unit/api/schema/mounts.rs"]
mod mount_tests;
