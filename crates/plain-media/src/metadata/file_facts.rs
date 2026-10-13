//! Per-file metadata facts read straight from the OS: canonical path,
//! bucket, size, timestamps, probe results and image dimensions.
//!
//! Read-only by construction — stat, one tag/container probe and one image
//! header read. No catalog, prefs, KV, index, watch or cache write, and no
//! id / favorite / trash / original-path field: those belong to the core
//! catalog that persists these rows.
//!
//! Error vs unknown: a path that cannot be resolved, is not a regular file,
//! is not expressible as UTF-8, or cannot be stat'ed or timestamped is an
//! `Err` — never a fabricated row. A fact the OS simply does not carry
//! (EXIF capture time, EXIF orientation outside JPEG, pixel dimensions of a
//! format the enabled `image` features cannot decode) is not an error: it
//! follows the existing unknown convention, `0` / `None` / empty string, so
//! a valid HEIC/AVIF still produces a row instead of leaving the scan.

use anyhow::{Context, Result, bail};
use plain_rs::db::{MediaFileMetadata, MediaFileMetadataAvailability, MediaFileMetadataRead};
use plain_rs::enums::DataType;
use std::fs::{File, Metadata};
use std::io::Read;
use std::path::Path;
use std::time::{Duration, SystemTime};

/// Leading window the JPEG APP1/EXIF segment lives in.
const EXIF_HEAD_BYTES: u64 = 128 * 1024;

/// Read the metadata of one media file into the row the catalog stores.
///
/// `data_type` must be one the media index covers (audio / video / image /
/// doc, as `DataType::media_type_str` names them).
pub fn read_file_metadata(path: &Path, data_type: DataType) -> Result<MediaFileMetadata> {
    Ok(read_file_metadata_with_availability(path, data_type)?.metadata)
}

pub fn read_file_metadata_with_availability(
    path: &Path,
    data_type: DataType,
) -> Result<MediaFileMetadataRead> {
    path.to_str().context("media path is not valid UTF-8")?;
    let kind = data_type
        .media_type_str()
        .with_context(|| format!("{data_type:?} is not a media data type"))?;
    let canonical = path
        .canonicalize()
        .with_context(|| format!("cannot resolve media path {}", path.display()))?;
    let meta = std::fs::metadata(&canonical)
        .with_context(|| format!("cannot stat {}", canonical.display()))?;
    if !meta.is_file() {
        bail!("{} is not a regular file", canonical.display());
    }
    let path_str = canonical
        .to_str()
        .with_context(|| format!("{} is not valid UTF-8", canonical.display()))?
        .to_string();
    let bucket_id = canonical
        .parent()
        .and_then(|p| p.to_str())
        .with_context(|| format!("{path_str} has no parent directory"))?
        .to_string();
    let size_bytes = i64::try_from(meta.len())
        .with_context(|| format!("{path_str} is larger than the row can store"))?;
    let modified = meta
        .modified()
        .with_context(|| format!("{path_str} has no readable modified time"))?;

    let probed = super::probe_media(&path_str, kind);
    let duration_ms = i64::from(probed.duration_secs)
        .checked_mul(1000)
        .context("duration in milliseconds does not fit the row")?;
    let (width, height, rotation) = image_facts(&canonical, kind);
    let available = MediaFileMetadataAvailability {
        tags: kind != "audio" || probed.parsed,
        duration: !matches!(kind, "audio" | "video") || probed.duration_available,
        dimensions: width > 0 && height > 0,
        rotation: width > 0 && height > 0 && rotation.is_some(),
        taken_at: false,
        album_file_id: false,
    };
    Ok(MediaFileMetadataRead {
        available,
        metadata: MediaFileMetadata {
            data_type,
            title: if probed.title.is_empty() {
                file_stem(&path_str)
            } else {
                probed.title
            },
            artist: probed.artist,
            path: path_str,
            bucket_id,
            size_bytes,
            duration_ms,
            album_file_id: String::new(),
            width,
            height,
            rotation: rotation.unwrap_or_default(),
            taken_at: None,
            created_at: created_at_of(birth_time(&meta), modified),
            updated_at: row_timestamp(modified),
        },
    })
}

/// File name without its extension, the title fallback a tagless file has
/// no better answer for.
fn file_stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_string()
}

/// UTC timestamp in the row format the rest of the database uses: the
/// catalog sorts `created_at`, `updated_at` and `taken_at` by their text
/// value, so the fixed-width `...Z` rendering of `dbtime` is what keeps
/// lexicographic order equal to chronological order inside a column.
fn row_timestamp(at: SystemTime) -> String {
    let millis = at
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|since| i64::try_from(since.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default();
    plain_rs::utils::dbtime::iso_from_unix_millis(millis)
}

/// Creation time, falling back to modified time on the filesystems that
/// record no birth time.
fn created_at_of(birth: Option<SystemTime>, modified: SystemTime) -> String {
    birth.map_or_else(|| row_timestamp(modified), row_timestamp)
}

fn birth_unix_secs(meta: &Metadata) -> Option<i64> {
    meta.created()
        .ok()
        .and_then(|created| created.duration_since(SystemTime::UNIX_EPOCH).ok())
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .filter(|seconds| *seconds > 0)
}

fn birth_time(meta: &Metadata) -> Option<SystemTime> {
    birth_unix_secs(meta).map(|secs| SystemTime::UNIX_EPOCH + Duration::from_secs(secs as u64))
}

/// Pixel and orientation facts. Every kind but image, and every format the
/// enabled `image` features cannot decode, reads as unknown (0/0/0/none).
fn image_facts(path: &Path, kind: &str) -> (i32, i32, Option<i32>) {
    if kind != "image" {
        return (0, 0, None);
    }
    let dimensions = image::ImageReader::open(path)
        .ok()
        .and_then(|reader| reader.into_dimensions().ok());
    let (width, height) = match dimensions {
        Some((w, h)) => (
            i32::try_from(w).unwrap_or_default(),
            i32::try_from(h).unwrap_or_default(),
        ),
        None => (0, 0),
    };
    (width, height, jpeg_rotation(path))
}

/// Clockwise rotation the EXIF orientation tag asks for. The mirrored
/// orientations (2/4/5/7) have no rotation-only equivalent and read as 0.
fn jpeg_rotation(path: &Path) -> Option<i32> {
    let mut head = Vec::new();
    if File::open(path)
        .and_then(|file| file.take(EXIF_HEAD_BYTES).read_to_end(&mut head))
        .is_err()
        || !head.starts_with(&[0xFF, 0xD8])
    {
        return None;
    }
    Some(match crate::previews::images::exif::orientation(&head)? {
        3 => 180,
        6 => 90,
        8 => 270,
        _ => 0,
    })
}

#[cfg(test)]
#[path = "../../tests/unit/metadata/file_facts.rs"]
mod tests;
