//! The media GraphQL roots — the file/media browsing surface in the
//! shared plain-rs API schema. Query and
//! mutation fields mirror the plain-app contract exactly (same names,
//! arguments and shapes), so a client written against one host works
//! against desktop or NAS.
//!
//! Hosts inject the backing services as async-graphql global data:
//! `Arc<crate::media::kv::Db>` (fjall store), `Arc<crate::prefs::Prefs>`,
//! `Arc<crate::db::Db>` and the caller's client id as
//! `String` (optional — file tasks key off it, empty for hosts without
//! sessions).

use async_graphql::{Context, ID, Object};
use std::sync::Arc;

use crate::db::Db as SqlDb;
use crate::media::fsx;
use crate::media::image_index::{self as media_index, MediaSort};
use crate::media::kv::{self, Db};
use crate::media::scan;
use crate::media::{file_tasks, trash};
use crate::prefs::Prefs;

mod mutation;
mod query;

pub use mutation::MediaMutationRoot;
pub use query::MediaQueryRoot;

pub use super::types::{
    ActionResult, Audio, Doc, DocExtGroup, File, FileSortBy, FileTask, FileTaskOpInput,
    FileTaskStatus, FileTaskType, Image, ImageSearchStatus, ImageSearchStatusType, Instant, Long,
    MediaBucket, MediaDataType, PathKind, ScanProgress, ScanState, Tag, TagRelation,
    TagRelationStub, TrashedFile, TrashedFileSortBy, TrashedFileType, Video,
};
pub use crate::enums::DataType;

type FieldResult<T> = async_graphql::Result<T>;

// ---------------------------------------------------------------------------
// Lazy tags on the media list types (plain-app DataLoader parity)
// ---------------------------------------------------------------------------

/// Test-only counter: how many times a tags field resolver actually hit
/// the tag store. Laziness tests reset it and assert 0 loads without a
/// `tags` selection.
#[cfg(test)]
pub(crate) static TAG_LOADS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[inline]
pub(crate) fn count_tag_load() {
    #[cfg(test)]
    TAG_LOADS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

fn tags_by_key(library: &SqlDb, key: &str, kind: i32) -> FieldResult<Vec<Tag>> {
    Ok(
        crate::library::tags::tags_for_key_of_kind(library, key, kind)?
            .into_iter()
            .map(|t| Tag {
                id: t.id.into(),
                name: t.name,
                r#type: t.kind,
                count: t.count,
            })
            .collect(),
    )
}
fn lazy_tags(ctx: &Context<'_>, id: &str, data_type: DataType) -> FieldResult<Vec<Tag>> {
    count_tag_load();
    tags_by_key(ctx.data::<Arc<SqlDb>>()?, id, data_type.kind())
}

#[async_graphql::ComplexObject]
impl Audio {
    /// Tags resolve lazily — only when the selection asks for the field,
    /// mirroring plain-app's DataLoader `dataProperty`.
    pub async fn tags(&self, ctx: &Context<'_>) -> FieldResult<Vec<Tag>> {
        lazy_tags(ctx, &self.id, DataType::Audio)
    }
}

#[async_graphql::ComplexObject]
impl Image {
    pub async fn tags(&self, ctx: &Context<'_>) -> FieldResult<Vec<Tag>> {
        lazy_tags(ctx, &self.id, DataType::Image)
    }
}

#[async_graphql::ComplexObject]
impl Video {
    pub async fn tags(&self, ctx: &Context<'_>) -> FieldResult<Vec<Tag>> {
        lazy_tags(ctx, &self.id, DataType::Video)
    }
}

#[async_graphql::ComplexObject]
impl Doc {
    pub async fn tags(&self, ctx: &Context<'_>) -> FieldResult<Vec<Tag>> {
        lazy_tags(ctx, &self.id, DataType::Doc)
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// The `text:` field of the shared search DSL, used by the paginated
/// list ops that filter on plain-app's server-side `text:` extraction.
/// Empty string = no filtering.
/// Last path segment, mirroring Kotlin `File.name` ("/" and "" → "").
pub fn file_name_of(path: &str) -> String {
    std::path::Path::new(path.trim_end_matches('/'))
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string()
}

pub fn file_entry_to_gql(e: fsx::FileEntry) -> File {
    File {
        name: file_name_of(&e.path),
        path: e.path,
        created_at: Some(Instant(e.created_at)),
        updated_at: Instant(e.updated_at),
        size: Long(e.size),
        is_dir: e.is_dir,
        child_count: e.child_count,
        media_id: None,
    }
}

pub fn task_to_gql(t: file_tasks::FileTask) -> FileTask {
    let kind = match t.kind {
        file_tasks::FileTaskType::Copy => FileTaskType::COPY,
        file_tasks::FileTaskType::Move => FileTaskType::MOVE,
    };
    let status = match t.status {
        file_tasks::FileTaskStatus::Queued => FileTaskStatus::QUEUED,
        file_tasks::FileTaskStatus::Running => FileTaskStatus::RUNNING,
        file_tasks::FileTaskStatus::Done => FileTaskStatus::DONE,
        file_tasks::FileTaskStatus::Error => FileTaskStatus::ERROR,
    };
    FileTask {
        id: t.id.into(),
        r#type: kind,
        title: t.title,
        status,
        error: t.error,
        total_bytes: Long(t.total_bytes),
        done_bytes: Long(t.done_bytes),
        total_items: i32::try_from(t.total_items).unwrap_or(i32::MAX),
        done_items: i32::try_from(t.done_items).unwrap_or(i32::MAX),
        created_at: Instant(t.created_at),
        updated_at: Instant(t.updated_at),
    }
}

/// GraphQL `FileSortBy` → media index sort order. TAKEN_AT_DESC only
/// applies to capture-date grouping (API_SPEC §3); the media index sorts
/// those by mtime-descending instead.
fn media_sort(sort_by: FileSortBy) -> MediaSort {
    match sort_by {
        FileSortBy::DATE_ASC => MediaSort::DateAsc,
        FileSortBy::DATE_DESC | FileSortBy::TAKEN_AT_DESC => MediaSort::DateDesc,
        FileSortBy::SIZE_ASC => MediaSort::SizeAsc,
        FileSortBy::SIZE_DESC => MediaSort::SizeDesc,
        FileSortBy::NAME_ASC => MediaSort::NameAsc,
        FileSortBy::NAME_DESC => MediaSort::NameDesc,
    }
}

/// The media bucket of a file is its containing directory.
fn bucket_of(path: &str) -> String {
    media_index::parent_dir_of(path)
}

/// Map a media-index row (kind `doc`) to the GraphQL `Doc` type.
/// Mirrors plain-app `DDoc.toDocModel` — title is the display name,
/// extension the lowercased filename extension.
fn doc_to_gql(r: media_index::MediaSearchResult) -> Doc {
    Doc {
        id: r.uuid.into(),
        title: r.name.clone(),
        path: r.path.clone(),
        extension: scan::ext_of(&r.name),
        size: Long(r.size),
        bucket_id: bucket_of(&r.path).into(),
        created_at: ts_to_instant(r.modified),
        updated_at: ts_to_instant(r.modified),
    }
}

/// Unix seconds → Instant (UTC), epoch when unknown.
pub fn ts_to_instant(secs: i64) -> Instant {
    Instant(
        chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0)
            .unwrap_or_else(|| chrono::DateTime::<chrono::Utc>::UNIX_EPOCH),
    )
}

fn scan_state_to_gql(state: scan::ScanState) -> ScanState {
    match state {
        scan::ScanState::Idle => ScanState::IDLE,
        scan::ScanState::Running => ScanState::RUNNING,
        scan::ScanState::Paused => ScanState::PAUSED,
        scan::ScanState::Stopped => ScanState::STOPPED,
    }
}

/// Convert a `crate::media::trash::TrashItem` into the GraphQL `File`
/// type:
///   * `path` = `<disk>/.nas-trash/<trash_rel_path>` (the physical trashed path)
///   * `is_dir` = (kind == "dir")
///   * `created_at` = `updated_at` = `deleted_at` (unix seconds → RFC3339)
///   * `size` = it.size.unwrap_or(0)
///   * `children` = entry_count - 1 when dir and entry_count > 1, else 0
fn trash_item_to_file(it: trash::TrashItem) -> File {
    let is_dir = it.kind == "dir";
    let trashed_path = format!(
        "{}/.nas-trash/{}",
        it.disk.trim_end_matches('/'),
        it.trash_rel_path
    );
    let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(it.deleted_at, 0)
        .unwrap_or_else(chrono::Utc::now);
    let ts = Instant(dt);
    let size = it.size.unwrap_or(0);
    let child_count = if is_dir {
        it.entry_count.map(|c| ((c - 1).max(0)) as i32).unwrap_or(0)
    } else {
        0
    };
    File {
        name: file_name_of(&trashed_path),
        path: trashed_path,
        created_at: Some(ts),
        updated_at: ts,
        size: Long(size),
        is_dir,
        child_count,
        media_id: None,
    }
}

/// Total metadata probe: `None` for blank/`.` paths, missing files and
/// stat errors — `pathExists`/`pathKind` never raise on those.
async fn stat_path(path: &str) -> Option<std::fs::Metadata> {
    if path.trim().is_empty() || path == "." {
        return None;
    }
    tokio::fs::metadata(path).await.ok()
}

/// i64 scanner counter → GraphQL Int. File counts can never reach 2^31 in
/// practice; the saturating cast keeps the mapping total.
fn count_to_i32(n: i64) -> i32 {
    n.clamp(0, i64::from(i32::MAX)) as i32
}

#[cfg(test)]
#[path = "../../../tests/unit/media/gql.rs"]
mod tests;
