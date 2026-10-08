//! Public `/graphql` file-browsing roots: mounts, a directory listing, a
//! single-path stat and its media probe.
//!
//! The directory walk itself is [`crate::filesystem`] — the same code the
//! app's `files/read` route runs, reached through
//! [`super::file_reads`] so the two cannot drift. Only the things Rust
//! genuinely cannot know reach for the host: which volumes are mounted,
//! what "recent" means on this platform, and image/video/audio metadata.

use super::public_facts::{flag, id, integer, list, rows, text};
use super::public_gate;
use super::{file_reads, host::Host};
use crate::content_types::{
    AudioFileInfo, DriveType, FavoriteFolder, File, FileInfo, FileSortBy, ImageFileInfo, Instant,
    Location, Long, MediaFileInfo, Mount, VideoFileInfo,
};
use crate::db::Db;
use crate::filesystem::{SortBy, browse::Request};
use async_graphql::{Context, Enum, Object};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::{path::Path, sync::Arc};

const STORAGE: &str = "WRITE_EXTERNAL_STORAGE";

#[derive(Default)]
pub struct FilesQuery;

#[Object]
impl FilesQuery {
    /// Volumes the platform reports. The host builds these because drive
    /// enumeration is platform state; there is nothing to gate.
    async fn mounts(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Mount>> {
        let facts = host_call(ctx, "systemMountFacts", json!({})).await?;
        Ok(rows(&facts, mount))
    }

    /// Recently-opened files. Reading them is a storage read, so this one
    /// gates — plain-app's `recentFiles` calls `checkEnabledAsync`.
    async fn recent_files(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<File>> {
        public_gate::require(prefs(ctx), &[STORAGE])?;
        let facts = host_call(ctx, "systemRecentFileFacts", json!({})).await?;
        Ok(rows(&facts, file))
    }

    /// One page of a directory listing, filtered and sorted by the search
    /// DSL. `query` may carry a `parent` field to redirect the root, which
    /// is what the file manager's breadcrumb links send.
    async fn files(
        &self,
        ctx: &Context<'_>,
        root: String,
        offset: i32,
        limit: i32,
        query: String,
        sort_by: FileSortBy,
    ) -> async_graphql::Result<Vec<File>> {
        public_gate::require(prefs(ctx), &[STORAGE])?;
        let page = file_reads::execute(
            host(ctx),
            Request {
                root,
                query,
                text: None,
                show_hidden: None,
                sort_by: sort_by.into(),
                offset: offset.max(0) as usize,
                limit: Some(limit.max(0) as usize),
                count_only: false,
            },
        )
        .await
        .map_err(async_graphql::Error::new)?;
        Ok(list(&page, "items", file))
    }

    /// Total matches for the same query, ignoring paging.
    async fn file_count(
        &self,
        ctx: &Context<'_>,
        root: String,
        query: String,
    ) -> async_graphql::Result<i32> {
        public_gate::require(prefs(ctx), &[STORAGE])?;
        let page = file_reads::execute(
            host(ctx),
            Request {
                root,
                query,
                text: None,
                show_hidden: None,
                sort_by: SortBy::NameAsc,
                offset: 0,
                limit: None,
                count_only: true,
            },
        )
        .await
        .map_err(async_graphql::Error::new)?;
        Ok(page["count"].as_u64().unwrap_or_default() as i32)
    }

    /// Stat plus, for media names, the platform's decoder output. A path
    /// that cannot be resolved is not an error: plain-app reports zeros so
    /// the info panel renders rather than blowing up on a stale link.
    ///
    /// The probe only runs once the stat succeeded — that stat is what the
    /// file-access authorization rides on, so probing a denied path would
    /// hand a client metadata for bytes it may not read.
    async fn file_info(
        &self,
        ctx: &Context<'_>,
        path: String,
        file_name: Option<String>,
    ) -> async_graphql::Result<FileInfo> {
        public_gate::require(prefs(ctx), &[STORAGE])?;
        let record = file_reads::stat_record(host(ctx), &path).await;
        let name = file_name.unwrap_or_else(|| {
            Path::new(&path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        let data = match &record {
            Some(_) => media_info(ctx, &path, &name).await?,
            None => None,
        };
        Ok(FileInfo {
            path,
            updated_at: record
                .as_ref()
                .map(|record| instant(record.updated_at))
                .unwrap_or_else(epoch),
            size: Long(record.map(|record| record.size).unwrap_or_default()),
            data,
        })
    }

    /// A total predicate, never an error: a blank path, `"."`, a missing
    /// file and a denied path all read as `false` so callers can use it in
    /// a filter without guarding every call.
    async fn path_exists(&self, ctx: &Context<'_>, path: String) -> async_graphql::Result<bool> {
        Ok(file_reads::stat_record(host(ctx), &path).await.is_some())
    }

    /// Same total-predicate semantics as `pathExists`; `null` when the path
    /// cannot be resolved.
    async fn path_kind(
        &self,
        ctx: &Context<'_>,
        path: String,
    ) -> async_graphql::Result<Option<PathKind>> {
        Ok(file_reads::stat_record(host(ctx), &path)
            .await
            .map(|record| {
                if record.is_dir {
                    PathKind::Dir
                } else {
                    PathKind::File
                }
            }))
    }

    /// Bookmarked directories. These live in the Rust SQLite
    /// `favorite_folders` table, so this reads the store directly instead
    /// of asking the platform to refresh — going through the host would
    /// re-enter this very root once `main_graphql` serves the public
    /// schema.
    async fn favorite_folders(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<FavoriteFolder>> {
        favorite_folders(ctx).await
    }
}

#[derive(Default)]
pub struct FavoritesMutation;

#[Object]
impl FavoritesMutation {
    /// Each mutation returns the whole updated list, so a client never has
    /// to guess what the operation did to the rows it did not touch.
    async fn add_favorite_folder(
        &self,
        ctx: &Context<'_>,
        root_path: String,
        full_path: String,
    ) -> async_graphql::Result<Vec<FavoriteFolder>> {
        crate::library::favorite_folders::add_full_path(db(ctx), &root_path, &full_path)
            .map_err(|error| async_graphql::Error::new(error.to_string()))?;
        favorite_folders(ctx).await
    }

    /// An unknown `fullPath` is not an error: the folder was already gone,
    /// and returning the current list is the honest answer either way.
    async fn remove_favorite_folder(
        &self,
        ctx: &Context<'_>,
        full_path: String,
    ) -> async_graphql::Result<Vec<FavoriteFolder>> {
        let db = db(ctx);
        if let Some(folder) = crate::library::favorite_folders::find_by_full_path(db, &full_path)
            .map_err(|error| async_graphql::Error::new(error.to_string()))?
        {
            crate::library::favorite_folders::remove(db, &folder.root_path, &folder.relative_path)
                .map_err(|error| async_graphql::Error::new(error.to_string()))?;
        }
        favorite_folders(ctx).await
    }

    async fn set_favorite_folder_alias(
        &self,
        ctx: &Context<'_>,
        full_path: String,
        alias: String,
    ) -> async_graphql::Result<Vec<FavoriteFolder>> {
        let db = db(ctx);
        if let Some(folder) = crate::library::favorite_folders::find_by_full_path(db, &full_path)
            .map_err(|error| async_graphql::Error::new(error.to_string()))?
        {
            crate::library::favorite_folders::set_alias(
                db,
                &folder.root_path,
                &folder.relative_path,
                &alias,
            )
            .map_err(|error| async_graphql::Error::new(error.to_string()))?;
        }
        favorite_folders(ctx).await
    }
}

async fn favorite_folders(ctx: &Context<'_>) -> async_graphql::Result<Vec<FavoriteFolder>> {
    let db = db(ctx);
    Ok(crate::library::favorite_folders::list(db)
        .map_err(|error| async_graphql::Error::new(error.to_string()))?
        .iter()
        .map(|folder| FavoriteFolder {
            root_path: folder.root_path.clone(),
            full_path: crate::library::favorite_folders::full_path_of(folder),
            alias: folder.alias.clone(),
        })
        .collect())
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
enum PathKind {
    File,
    Dir,
}

impl From<FileSortBy> for SortBy {
    fn from(sort_by: FileSortBy) -> Self {
        match sort_by {
            FileSortBy::DateAsc => SortBy::DateAsc,
            FileSortBy::DateDesc => SortBy::DateDesc,
            FileSortBy::SizeAsc => SortBy::SizeAsc,
            FileSortBy::SizeDesc => SortBy::SizeDesc,
            FileSortBy::NameAsc | FileSortBy::TakenAtDesc => SortBy::NameAsc,
            FileSortBy::NameDesc => SortBy::NameDesc,
        }
    }
}

/// The probe is chosen by file *name*, not by sniffing: the client may pass
/// a display name that differs from the on-disk one, and plain-app keys off
/// the extension either way.
async fn media_info(
    ctx: &Context<'_>,
    path: &str,
    name: &str,
) -> async_graphql::Result<Option<MediaFileInfo>> {
    let extension = Path::new(name)
        .extension()
        .map(|extension| extension.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let method = if is_image(&extension) {
        "systemImageFileInfo"
    } else if is_video(&extension) {
        "systemVideoFileInfo"
    } else if is_audio(&extension) {
        "systemAudioFileInfo"
    } else {
        return Ok(None);
    };
    let facts = host_call(ctx, method, json!({ "path": path })).await?;
    if facts.is_null() {
        return Ok(None);
    }
    Ok(Some(match method {
        "systemImageFileInfo" => MediaFileInfo::Image(ImageFileInfo {
            width: integer(&facts, "width") as i32,
            height: integer(&facts, "height") as i32,
            location: location(&facts),
        }),
        "systemVideoFileInfo" => MediaFileInfo::Video(VideoFileInfo {
            width: integer(&facts, "width") as i32,
            height: integer(&facts, "height") as i32,
            duration_ms: Long(integer(&facts, "durationMs")),
            location: location(&facts),
        }),
        _ => MediaFileInfo::Audio(AudioFileInfo {
            duration_ms: Long(integer(&facts, "durationMs")),
            location: location(&facts),
        }),
    }))
}

fn location(value: &Value) -> Option<Location> {
    let raw=&value["location"];
    if let (Some(latitude),Some(longitude))=(raw["latitude"].as_f64(),raw["longitude"].as_f64()){return Some(Location{latitude,longitude});}
    let raw=value["rawLocation"].as_str()?;
    static PATTERN:std::sync::OnceLock<regex::Regex>=std::sync::OnceLock::new();
    let matched=PATTERN.get_or_init(||regex::Regex::new(r"([+\-]\d{1,3}\.\d{4})([+\-]\d{1,3}\.\d{4})").unwrap()).captures(raw)?;
    Some(Location{latitude:matched[1].parse().ok()?,longitude:matched[2].parse().ok()?})
}

/// Kept in step with plain-app's `isImageFast` / `isVideoFast` /
/// `isAudioFast`, which are extension checks over the same extension sets
/// the platform decoders register.
fn is_image(extension: &str) -> bool {
    matches!(
        extension,
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "heic" | "heif" | "svg"
    )
}

fn is_video(extension: &str) -> bool {
    matches!(
        extension,
        "mp4" | "m4v" | "mov" | "mkv" | "webm" | "avi" | "3gp" | "ts" | "flv" | "wmv"
    )
}

fn is_audio(extension: &str) -> bool {
    matches!(
        extension,
        "mp3" | "m4a" | "aac" | "wav" | "flac" | "ogg" | "opus" | "amr" | "wma" | "mid"
    )
}

fn file(item: &Value) -> File {
    File {
        media_id: item["mediaId"]
            .as_str()
            .filter(|value| !value.is_empty())
            .map(|value| async_graphql::ID::from(value)),
        name: text(item, "name"),
        path: text(item, "path"),
        created_at: item["createdAt"].as_i64().map(|millis| instant(millis)),
        updated_at: instant(item["updatedAt"].as_i64().unwrap_or_default()),
        size: Long(item["size"].as_i64().unwrap_or_default()),
        is_dir: flag(item, "isDir"),
        child_count: integer(item, "childCount") as i32,
    }
}

fn mount(item: &Value) -> Mount {
    Mount {
        id: async_graphql::ID(format!("path:{}",text(item,"path"))),
        name: text(item, "name"),
        path: text(item, "path"),
        mount_point: text(item, "path"),
        fs_type: text(item, "fsType"),
        total_bytes: Long(integer(item, "totalBytes")),
        used_bytes: Long(integer(item, "totalBytes").saturating_sub(integer(item,"freeBytes")).max(0)),
        free_bytes: Long(integer(item, "freeBytes")),
        remote: flag(item, "remote"),
        alias: text(item, "alias"),
        drive_type: DriveType::parse(&text(item, "driveType")),
        disk_id: text(item, "diskId"),
    }
}

fn instant(millis: i64) -> Instant {
    Instant(DateTime::<Utc>::from_timestamp_millis(millis).unwrap_or_else(epoch_datetime))
}

fn epoch() -> Instant {
    Instant(epoch_datetime())
}

fn epoch_datetime() -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(0, 0).unwrap_or_default()
}

fn host<'a>(ctx: &'a Context<'_>) -> &'a Arc<Host> {
    ctx.data_unchecked::<Arc<Host>>()
}

fn prefs<'a>(ctx: &'a Context<'_>) -> &'a Arc<crate::prefs::Prefs> {
    ctx.data_unchecked::<Arc<crate::prefs::Prefs>>()
}

fn db<'a>(ctx: &'a Context<'_>) -> &'a Arc<Db> {
    ctx.data_unchecked::<Arc<Db>>()
}

async fn host_call(ctx: &Context<'_>, method: &str, params: Value) -> async_graphql::Result<Value> {
    host(ctx)
        .call(method, params)
        .await
        .map_err(async_graphql::Error::new)
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/public_files.rs"]
mod tests;
