//! The media GraphQL roots — the file/media browsing surface shared by
//! plain-nas's NAS schema and the desktop api schema. Query and
//! mutation fields mirror the plain-app contract exactly (same names,
//! arguments and shapes), so a client written against one host works
//! against the other.
//!
//! Hosts inject the backing services as async-graphql global data:
//! `Arc<crate::media::kv::Db>` (fjall store), `Arc<crate::prefs::Prefs>`,
//! `Arc<crate::library::db::LibraryDb>` and the caller's client id as
//! `String` (optional — file tasks key off it, empty for hosts without
//! sessions).

pub mod types;

use async_graphql::{Context, ID, Object};
use std::sync::Arc;

use crate::library::db::LibraryDb;
use crate::media::fsx;
use crate::media::image_index::{self as media_index, MediaSort};
use crate::media::kv::{self, Db};
use crate::media::scan;
use crate::media::{file_tasks, trash};
use crate::prefs::Prefs;

pub use crate::enums::DataType;
pub use types::{
    ActionResult, Audio, Doc, DocExtGroup, File, FileSortBy, FileTask, FileTaskOpInput,
    FileTaskStatus, FileTaskType, Image, ImageSearchStatus, ImageSearchStatusType, Instant, Long,
    MediaBucket, MediaDataType, PathKind, ScanProgress, ScanState, Tag, TagRelation,
    TagRelationStub, TrashItem, TrashItemType, TrashSortBy, Video,
};

type FieldResult<T> = async_graphql::Result<T>;

fn selected_scan_roots(root: String, source_dirs: &[String]) -> Vec<std::path::PathBuf> {
    if root == "/" {
        let sources: Vec<_> = source_dirs
            .iter()
            .filter(|dir| !dir.is_empty())
            .map(std::path::PathBuf::from)
            .collect();
        if !sources.is_empty() {
            return sources;
        }
    }
    vec![std::path::PathBuf::from(root)]
}

/// Run blocking work (file probes, fjall batches) off the async runtime.
/// Metadata hydration parses files — a lofty read can touch a whole MP3 —
/// and must never occupy a tokio worker thread.
async fn run_blocking<T, F>(f: F) -> FieldResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| async_graphql::Error::new(format!("blocking task failed: {e}")))
}

// ---------------------------------------------------------------------------
// Query root
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct MediaQueryRoot;

#[Object]
impl MediaQueryRoot {
    /// Count of entries in a directory (non-recursive). Mirrors plain-app
    /// `fileCount(root, query)`: `root` scopes the count (empty/`/` = whole
    /// tree). When `trash:true` is in the query, returns the global trash count.
    async fn file_count(&self, root: String, query: String) -> FieldResult<i32> {
        let fields = crate::utils::search_dsl::parse(&query);
        let mut root_path = root;
        let mut relative_path = String::new();
        let mut show_hidden = false;
        let mut trash_only = false;
        for f in &fields {
            match f.name.as_str() {
                "root_path" => root_path = f.value.clone(),
                "relative_path" => relative_path = f.value.clone(),
                "show_hidden" => show_hidden = f.value == "true",
                "trash" => trash_only = f.value == "true",
                _ => {}
            }
        }
        if trash_only {
            return Ok(trash::trash_count()? as i32);
        }
        let base = if relative_path.is_empty() {
            if root_path.is_empty() {
                "/".to_string()
            } else {
                root_path
            }
        } else {
            let rel = relative_path.trim_start_matches('/');
            format!(
                "{}/{}",
                if root_path.is_empty() {
                    "/"
                } else {
                    root_path.trim_end_matches('/')
                },
                rel
            )
        };
        let p = std::path::Path::new(&base);
        if !p.is_dir() {
            return Ok(0);
        }
        match fsx::count_dir_entries(p, show_hidden) {
            Ok(n) => Ok(n as i32),
            Err(_) => Ok(0),
        }
    }

    /// Recent files (most-recent first). Mirrors Go `recentFiles()` which
    /// loads up to 500 paths and filters to those that still exist on disk.
    async fn recent_files(&self, ctx: &Context<'_>) -> FieldResult<Vec<File>> {
        let prefs = ctx.data::<Arc<Prefs>>()?;
        let paths = kv::recent::get_recent_files(prefs, 500);
        let mut out = Vec::new();
        for p in paths {
            let path = std::path::Path::new(&p);
            if let Ok(entry) = fsx::stat(path).await {
                out.push(file_entry_to_gql(entry));
            }
        }
        Ok(out)
    }

    /// File listing. Mirrors plain-app `files(root, offset, limit, query, sortBy)`
    /// (Kotlin `FileSystemHelper.search`): the effective directory is
    /// `parent` from the query DSL, falling back to the `root` argument,
    /// then to the legacy Go-dialect `root_path` DSL field; `relative_path`
    /// (legacy) is joined underneath. `text` → recursive tantivy search,
    /// `trash:true` → trash listing.
    async fn files(
        &self,
        root: String,
        offset: i32,
        limit: i32,
        query: String,
        sort_by: FileSortBy,
    ) -> FieldResult<Vec<File>> {
        let fields = crate::utils::search_dsl::parse(&query);
        let mut root_path = String::new();
        let mut parent = String::new();
        let mut relative_path = String::new();
        let mut text = String::new();
        let mut show_hidden = false;
        let mut trash_only = false;
        for f in &fields {
            match f.name.as_str() {
                "root_path" => root_path = f.value.clone(),
                "parent" => parent = f.value.clone(),
                "relative_path" => relative_path = f.value.clone(),
                "text" => text = f.value.clone(),
                "show_hidden" => show_hidden = f.value == "true",
                "trash" => trash_only = f.value == "true",
                _ => {}
            }
        }

        // Trash listing path: when `trash:true` is set, delegate to the
        // trash index. The `text` filter is applied inside `list_trash`.
        if trash_only {
            let order = match sort_by {
                FileSortBy::DATE_ASC => trash::SortOrder::DeletedAtOldest,
                // TAKEN_AT_DESC only applies to capture-date grouping
                // (API_SPEC §3); for a plain listing it falls back to the
                // default newest-first order.
                FileSortBy::DATE_DESC | FileSortBy::TAKEN_AT_DESC => {
                    trash::SortOrder::DeletedAtNewest
                }
                FileSortBy::NAME_ASC => trash::SortOrder::NameAsc,
                FileSortBy::NAME_DESC => trash::SortOrder::NameDesc,
                FileSortBy::SIZE_ASC => trash::SortOrder::SizeAsc,
                FileSortBy::SIZE_DESC => trash::SortOrder::SizeDesc,
            };
            let items =
                trash::list_trash(offset.max(0) as usize, limit.max(1) as usize, &text, order)?;
            return Ok(items.into_iter().map(trash_item_to_file).collect());
        }

        // Effective directory: `parent` overrides `root` (plain-app
        // `parent.ifEmpty { root }`), then the legacy Go-dialect
        // `root_path` DSL field; `relative_path` (legacy) joins underneath.
        let base = files_base_dir(&parent, &root, &root_path, &relative_path);

        let offset = offset.max(0) as usize;
        let limit = if limit <= 0 { 1000 } else { limit as usize };

        // Map FileSortBy to fsx::SortBy. TAKEN_AT_DESC has no filesystem
        // equivalent (plain files carry no capture time) and falls back to
        // mtime-descending.
        let sort = match sort_by {
            FileSortBy::NAME_ASC => fsx::SortBy::NameAsc,
            FileSortBy::NAME_DESC => fsx::SortBy::NameDesc,
            FileSortBy::DATE_ASC => fsx::SortBy::DateAsc,
            FileSortBy::DATE_DESC | FileSortBy::TAKEN_AT_DESC => fsx::SortBy::DateDesc,
            FileSortBy::SIZE_ASC => fsx::SortBy::SizeAsc,
            FileSortBy::SIZE_DESC => fsx::SortBy::SizeDesc,
        };

        // If text filter is set, use search index (tantivy).
        if !text.trim().is_empty() {
            let paths = crate::media::search::search_index_files(
                &text,
                &base,
                offset,
                limit,
                show_hidden,
                "",
                0,
            )?;
            let mut out: Vec<File> = Vec::new();
            for sf in paths {
                let p = std::path::Path::new(&sf.path);
                match fsx::stat(p).await {
                    Ok(entry) => out.push(file_entry_to_gql(entry)),
                    Err(_) => continue,
                }
            }
            return Ok(out);
        }

        // No text filter → single-directory listing (fast path).
        // For NAME_* sorts, use paged listing (avoids stat on all entries).
        let entries = match sort {
            fsx::SortBy::NameAsc | fsx::SortBy::NameDesc => {
                fsx::list_dir_paged(
                    std::path::Path::new(&base),
                    show_hidden,
                    offset,
                    limit,
                    sort,
                )
                .await
            }
            _ => {
                // For DATE_*/SIZE_* sorts, list all then sort+paginate.
                let mut entries = fsx::list_dir(std::path::Path::new(&base), show_hidden).await;
                // Sort: dirs first, then by the requested key.
                match sort {
                    fsx::SortBy::DateAsc => {
                        entries.sort_by(|a, b| {
                            match a.is_dir.cmp(&b.is_dir).reverse() {
                                std::cmp::Ordering::Equal => {}
                                ord => return ord,
                            }
                            a.updated_at.cmp(&b.updated_at)
                        });
                    }
                    fsx::SortBy::DateDesc => {
                        entries.sort_by(|a, b| {
                            match a.is_dir.cmp(&b.is_dir).reverse() {
                                std::cmp::Ordering::Equal => {}
                                ord => return ord,
                            }
                            b.updated_at.cmp(&a.updated_at)
                        });
                    }
                    fsx::SortBy::SizeAsc => {
                        entries.sort_by(|a, b| {
                            match a.is_dir.cmp(&b.is_dir).reverse() {
                                std::cmp::Ordering::Equal => {}
                                ord => return ord,
                            }
                            a.size.cmp(&b.size)
                        });
                    }
                    fsx::SortBy::SizeDesc => {
                        entries.sort_by(|a, b| {
                            match a.is_dir.cmp(&b.is_dir).reverse() {
                                std::cmp::Ordering::Equal => {}
                                ord => return ord,
                            }
                            b.size.cmp(&a.size)
                        });
                    }
                    _ => {}
                }
                // Apply offset/limit.
                if offset >= entries.len() {
                    return Ok(Vec::new());
                }
                let end = (offset + limit).min(entries.len());
                entries[offset..end].to_vec()
            }
        };

        Ok(entries.into_iter().map(file_entry_to_gql).collect())
    }

    /// Total number of trashed items.
    async fn trash_count(&self) -> FieldResult<i32> {
        Ok(trash::trash_count()? as i32)
    }

    /// Paginated trash listing. The DSL `text:` field of `query` filters
    /// the entries (blank = no filter); pass `DATE_DESC` for the usual
    /// newest-first view — the argument is required, there is no server
    /// default (API_SPEC §3).
    async fn trash_items(
        &self,
        offset: i32,
        limit: i32,
        query: String,
        sort_by: TrashSortBy,
    ) -> FieldResult<Vec<TrashItem>> {
        let order = match sort_by {
            TrashSortBy::DATE_DESC => trash::SortOrder::DeletedAtNewest,
            TrashSortBy::DATE_ASC => trash::SortOrder::DeletedAtOldest,
            TrashSortBy::NAME_ASC => trash::SortOrder::NameAsc,
            TrashSortBy::NAME_DESC => trash::SortOrder::NameDesc,
            TrashSortBy::SIZE_ASC => trash::SortOrder::SizeAsc,
            TrashSortBy::SIZE_DESC => trash::SortOrder::SizeDesc,
        };
        let items =
            trash::list_trash(offset.max(0) as usize, limit.max(1) as usize, &query, order)?;
        Ok(items
            .into_iter()
            .filter_map(|mut it| {
                let _ = trash::fill_stats(&mut it);
                let display_name = std::path::Path::new(&it.original_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                let trashed_path =
                    format!("{}/{}", it.disk.trim_end_matches('/'), it.trash_rel_path);
                Some(TrashItem {
                    // Rows with an unknown kind (older build) are skipped,
                    // not migrated — stale derived data heals by attrition.
                    r#type: TrashItemType::from_kind(&it.kind)?,
                    id: it.id.into(),
                    original_path: it.original_path,
                    disk: it.disk,
                    trash_rel_path: it.trash_rel_path,
                    deleted_at: ts_to_instant(it.deleted_at),
                    uid: it.uid as i32,
                    gid: it.gid as i32,
                    mode: it.mode as i32,
                    size_bytes: it.size.map(Long),
                    entry_count: it.entry_count.map(|n| n.min(i32::MAX as i64) as i32),
                    display_name,
                    trashed_path,
                })
            })
            .collect())
    }

    /// Look up tag relations for entity keys (media ids / entity keys),
    /// filtered to the given data type. Mirrors plain-app `tagRelations`;
    /// backs the client-side lazy tag join.
    async fn tag_relations(
        &self,
        ctx: &Context<'_>,
        #[graphql(name = "type")] r#type: DataType,
        keys: Vec<String>,
    ) -> FieldResult<Vec<TagRelation>> {
        let library = ctx.data::<Arc<LibraryDb>>()?;
        let kind = r#type.kind();
        let out = crate::library::tags::relations_for_keys_of_kind(library, &keys, kind)
            .into_iter()
            .map(|rel| TagRelation {
                tag_id: rel.tag_id.into(),
                key: rel.key,
            })
            .collect();
        Ok(out)
    }

    /// List tags for the given data type.
    async fn tags(&self, ctx: &Context<'_>, r#type: DataType) -> FieldResult<Vec<Tag>> {
        let library = ctx.data::<Arc<LibraryDb>>()?;
        let kind = r#type.kind();
        let tags = crate::library::tags::tags_by_type(library, kind);
        Ok(tags
            .into_iter()
            .map(|t| Tag {
                id: t.id.into(),
                name: t.name,
                r#type: t.kind,
                count: t.count,
            })
            .collect())
    }

    /// List async file tasks for the current client.
    async fn file_tasks(&self, ctx: &Context<'_>) -> FieldResult<Vec<FileTask>> {
        let cid = ctx.data::<String>().cloned().unwrap_or_default();
        let tasks = file_tasks::list_tasks(&cid)?;
        Ok(tasks.into_iter().map(task_to_gql).collect())
    }

    /// Audio list (paginated, backed by the tantivy media search index).
    async fn audios(
        &self,
        ctx: &Context<'_>,
        offset: i32,
        limit: i32,
        query: String,
        sort_by: FileSortBy,
    ) -> FieldResult<Vec<Audio>> {
        let mut rows = media_index::global().search(
            &query,
            Some("audio"),
            None,
            media_sort(sort_by),
            offset.max(0) as usize,
            limit.clamp(1, 500) as usize,
        )?;
        // Lazy metadata hydration (probe duration / artist / title once
        // per file for what this page returns, persisted so later reads
        // serve it straight from the index). File parses run off the
        // async runtime.
        if let Ok(db) = ctx.data::<Arc<Db>>().cloned() {
            rows = run_blocking(move || {
                scan::hydrate_search_page(&db, &mut rows, true);
                rows
            })
            .await?;
        }
        Ok(rows
            .into_iter()
            .map(|r| Audio {
                id: r.uuid.into(),
                title: if r.title.is_empty() {
                    r.name.clone()
                } else {
                    r.title
                },
                artist: r.artist,
                path: r.path.clone(),
                duration_ms: Long(r.duration_secs as i64 * 1000),
                size: Long(r.size),
                bucket_id: bucket_of(&r.path).into(),
                album_file_id: String::new(),
                created_at: ts_to_instant(r.modified),
                updated_at: ts_to_instant(r.modified),
                is_favorite: false,
            })
            .collect())
    }

    /// Audio count (tantivy counting collector).
    async fn audio_count(&self, query: String) -> FieldResult<i32> {
        Ok(media_count(&query, Some("audio")))
    }

    /// Current media-index scan progress; same payload as the
    /// `media:scan:progress` WS push, for initial render and polling.
    async fn scan_progress(&self) -> FieldResult<ScanProgress> {
        let (indexed, total, state) = scan::scanner().get_progress();
        Ok(ScanProgress {
            indexed: count_to_i32(indexed),
            pending: count_to_i32((total - indexed).max(0)),
            total: count_to_i32(total),
            state: scan_state_to_gql(state),
        })
    }

    /// AI image search is a phone-app capability (on-device CLIP); the NAS
    /// answers with a well-formed `UNAVAILABLE` so probing clients render
    /// their "not available" path instead of a GraphQL error.
    async fn image_search_status(&self) -> FieldResult<ImageSearchStatus> {
        Ok(ImageSearchStatus {
            status: ImageSearchStatusType::Unavailable,
            download_progress: 0,
            error_message: String::new(),
            model_size: Long(0),
            model_dir: String::new(),
            is_indexing: false,
            total_images: 0,
            indexed_images: 0,
        })
    }

    /// The source directories the media scanner indexes.
    async fn media_source_dirs(&self, ctx: &Context<'_>) -> FieldResult<Vec<String>> {
        let prefs = ctx.data::<Arc<Prefs>>()?;
        Ok(kv::media_source::get(prefs))
    }

    /// Whether the path exists. Blank and '.' paths are `false`; stat
    /// errors (e.g. permission denied) count as "not there" — this query
    /// is a total predicate and never raises.
    async fn path_exists(&self, path: String) -> FieldResult<bool> {
        Ok(stat_path(&path).await.is_some())
    }

    /// Kind of the path: `FILE` or `DIR`; null when the path does not
    /// exist (same total-predicate semantics as `pathExists`).
    async fn path_kind(&self, path: String) -> FieldResult<Option<PathKind>> {
        Ok(match stat_path(&path).await {
            Some(m) if m.is_dir() => Some(PathKind::DIR),
            Some(_) => Some(PathKind::FILE),
            None => None,
        })
    }

    /// Image list (paginated, backed by the tantivy media search index).
    async fn images(
        &self,
        offset: i32,
        limit: i32,
        query: String,
        sort_by: FileSortBy,
    ) -> FieldResult<Vec<Image>> {
        let rows = media_index::global().search(
            &query,
            Some("image"),
            None,
            media_sort(sort_by),
            offset.max(0) as usize,
            limit.clamp(1, 500) as usize,
        )?;
        Ok(rows
            .into_iter()
            .map(|r| Image {
                id: r.uuid.into(),
                title: if r.title.is_empty() {
                    r.name.clone()
                } else {
                    r.title
                },
                path: r.path.clone(),
                size: Long(r.size),
                bucket_id: bucket_of(&r.path).into(),
                taken_at: Some(ts_to_instant(r.modified)),
                created_at: ts_to_instant(r.modified),
                updated_at: ts_to_instant(r.modified),
                is_favorite: false,
            })
            .collect())
    }

    /// Image count (tantivy counting collector).
    async fn image_count(&self, query: String) -> FieldResult<i32> {
        Ok(media_count(&query, Some("image")))
    }

    /// Video list (paginated, backed by the tantivy media search index).
    async fn videos(
        &self,
        ctx: &Context<'_>,
        offset: i32,
        limit: i32,
        query: String,
        sort_by: FileSortBy,
    ) -> FieldResult<Vec<Video>> {
        let mut rows = media_index::global().search(
            &query,
            Some("video"),
            None,
            media_sort(sort_by),
            offset.max(0) as usize,
            limit.clamp(1, 500) as usize,
        )?;
        // Lazy duration hydration — see `audios`.
        if let Ok(db) = ctx.data::<Arc<Db>>().cloned() {
            rows = run_blocking(move || {
                scan::hydrate_search_page(&db, &mut rows, false);
                rows
            })
            .await?;
        }
        Ok(rows
            .into_iter()
            .map(|r| Video {
                id: r.uuid.into(),
                title: if r.title.is_empty() {
                    r.name.clone()
                } else {
                    r.title
                },
                path: r.path.clone(),
                duration_ms: Long(r.duration_secs as i64 * 1000),
                size: Long(r.size),
                bucket_id: bucket_of(&r.path).into(),
                taken_at: Some(ts_to_instant(r.modified)),
                created_at: ts_to_instant(r.modified),
                updated_at: ts_to_instant(r.modified),
                is_favorite: false,
            })
            .collect())
    }

    /// Video count (tantivy counting collector).
    async fn video_count(&self, query: String) -> FieldResult<i32> {
        Ok(media_count(&query, Some("video")))
    }

    /// Document list (paginated, backed by the tantivy media search index).
    /// Docs need no metadata hydration, so this maps index rows directly.
    async fn docs(
        &self,
        offset: i32,
        limit: i32,
        query: String,
        sort_by: FileSortBy,
    ) -> FieldResult<Vec<Doc>> {
        let rows = media_index::global().search(
            &query,
            Some("doc"),
            None,
            media_sort(sort_by),
            offset.max(0) as usize,
            limit.clamp(1, 500) as usize,
        )?;
        Ok(rows.into_iter().map(doc_to_gql).collect())
    }

    /// Document count (tantivy counting collector).
    async fn doc_count(&self, query: String) -> FieldResult<i32> {
        Ok(media_count(&query, Some("doc")))
    }

    /// Per-extension group counts for the docs sidebar (no query filter,
    /// plain-app semantics — includes trashed, matching `docCount("")`).
    async fn doc_ext_groups(&self) -> FieldResult<Vec<DocExtGroup>> {
        let groups = tokio::task::spawn_blocking(|| media_index::global().doc_ext_groups())
            .await
            .map_err(|e| async_graphql::Error::new(format!("join: {e}")))?
            .map_err(|e| async_graphql::Error::new(format!("docExtGroups: {e}")))?;
        Ok(groups
            .into_iter()
            .map(|(ext, count)| DocExtGroup {
                ext: ext.to_uppercase(),
                count: count.clamp(0, i32::MAX as i64) as i32,
            })
            .collect())
    }

    /// `mediaBuckets(type)` mirrors Go `listMediaBuckets`: group media files
    /// by their containing directory. Backed by the KV bucket counters the scan pipeline
    /// maintains inside its write batches; cover items are one per-bucket
    /// tantivy query, run on a blocking thread so a large bucket count
    /// never stalls the async runtime.
    async fn media_buckets(
        &self,
        ctx: &Context<'_>,
        r#type: MediaDataType,
    ) -> FieldResult<Vec<MediaBucket>> {
        let kind = match r#type {
            MediaDataType::AUDIO => "audio".to_string(),
            MediaDataType::VIDEO => "video".to_string(),
            MediaDataType::IMAGE => "image".to_string(),
            MediaDataType::DOC => "doc".to_string(),
        };
        let db = ctx.data::<Arc<Db>>()?.clone();
        let buckets = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            let buckets = scan::list_buckets(&db, &kind)?;
            let wanted: std::collections::HashSet<String> =
                buckets.iter().map(|b| b.dir.clone()).collect();
            let tops = media_index::global()
                .bucket_top_items(&kind, &wanted, 4)
                .unwrap_or_default();
            Ok((buckets, tops))
        })
        .await
        .map_err(|e| async_graphql::Error::new(format!("join error: {e}")))??;
        let (buckets, tops) = buckets;
        Ok(buckets
            .into_iter()
            .map(|b| MediaBucket {
                id: b.dir.clone().into(),
                name: b
                    .dir
                    .rsplit('/')
                    .find(|s| !s.is_empty())
                    .unwrap_or(&b.dir)
                    .to_string(),
                item_count: b.item_count.min(i32::MAX as i64) as i32,
                top_item_paths: tops.get(&b.dir).cloned().unwrap_or_default(),
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Mutation root
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct MediaMutationRoot;

#[Object]
impl MediaMutationRoot {
    /// Create a directory. Mirrors Go `createDirModel` which returns the
    /// freshly created `*model.File`. The path is sandboxed via
    /// `fsx::safe_join` against the configured base directory.
    async fn create_dir(&self, path: String) -> FieldResult<File> {
        let p = std::path::Path::new(&path);
        fsx::ensure_dir(p).await?;
        let entry = fsx::stat(p).await?;
        Ok(file_entry_to_gql(entry))
    }

    /// Write a text file. Mirrors Go `writeTextFileModel` (2 MiB cap,
    /// overwrite-or-error on existing). Returns the resulting `File!`.
    async fn write_text_file(
        &self,
        path: String,
        content: String,
        overwrite: bool,
    ) -> FieldResult<File> {
        const MAX_BYTES: usize = 2 * 1024 * 1024;
        if content.len() > MAX_BYTES {
            return Err(async_graphql::Error::new("content too large"));
        }
        let p = std::path::Path::new(&path);
        if let Some(parent) = p.parent() {
            let parent_meta = tokio::fs::metadata(parent)
                .await
                .map_err(|e| async_graphql::Error::new(format!("parent: {e}")))?;
            if !parent_meta.is_dir() {
                return Err(async_graphql::Error::new("parent is not a directory"));
            }
        }
        match tokio::fs::metadata(p).await {
            Ok(m) if m.is_dir() => return Err(async_graphql::Error::new("path is a directory")),
            Ok(_) if !overwrite => return Err(async_graphql::Error::new("target exists")),
            _ => {}
        }
        tokio::fs::write(p, content.as_bytes())
            .await
            .map_err(|e| async_graphql::Error::new(format!("write: {e}")))?;
        let entry = fsx::stat(p).await?;
        Ok(file_entry_to_gql(entry))
    }

    /// Rename a file or directory in place: `path` is the current full path, `name` the new base name. Errors (missing target, name clash) surface as GraphQL errors.
    async fn rename_file(&self, path: String, name: String) -> FieldResult<bool> {
        let p = std::path::PathBuf::from(&path);
        let parent = p
            .parent()
            .ok_or_else(|| async_graphql::Error::new("bad path"))?;
        let new_path = parent.join(&name);
        fsx::rename(&p, &new_path).await?;
        Ok(true)
    }

    /// Synchronous single-file copy (path-addressed). `overwrite: false` fails when `dst` exists; for batches or large trees use `createCopyTask`.
    async fn copy_file(&self, src: String, dst: String, overwrite: bool) -> FieldResult<bool> {
        fsx::copy_path(
            std::path::Path::new(&src),
            std::path::Path::new(&dst),
            overwrite,
        )
        .await?;
        Ok(true)
    }

    /// Synchronous single-file move (path-addressed). `overwrite: false` fails when `dst` exists; for batches or large trees use `createMoveTask`.
    async fn move_file(&self, src: String, dst: String, overwrite: bool) -> FieldResult<bool> {
        fsx::move_path(
            std::path::Path::new(&src),
            std::path::Path::new(&dst),
            overwrite,
        )
        .await?;
        Ok(true)
    }

    /// Delete files by path (phone contract): returns how many were
    /// actually removed — per-path best-effort, hard failures are logged
    /// and skipped instead of failing the whole batch.
    async fn delete_files(
        &self,
        ctx: &Context<'_>,
        paths: Vec<String>,
    ) -> FieldResult<ActionResult> {
        let db = ctx.data::<Arc<Db>>()?;
        let mut affected = 0i32;
        for raw in paths {
            let p = std::path::Path::new(&raw);
            // Refuse to delete the filesystem root.
            if raw.trim() == "/" {
                return Err(async_graphql::Error::new("refusing to delete root"));
            }
            // Per-path best-effort: if it was a media file, drop the index
            // entry; if it was a directory tree, purge all matching
            // `media:path:` rows.
            let _ = scan::delete_by_path(db, &raw);
            // Even on a regular file we try the prefix purge so stale
            // directory entries are cleared.
            let _ = scan::delete_by_path_prefix(db, &raw);
            // `fsx::remove` is idempotent on missing paths, so existence
            // decides whether this path counts as affected.
            let existed = tokio::fs::symlink_metadata(p).await.is_ok();
            match fsx::remove(p).await {
                Ok(()) if existed => affected += 1,
                Ok(()) => {}
                Err(e) => {
                    log::debug!("[gql] deleteFiles {raw}: {e}");
                }
            }
        }
        Ok(ActionResult {
            affected_count: affected,
        })
    }

    /// Move each path to its disk-local `.nas-trash`. Batch destructive
    /// op → `ActionResult` per API_SPEC §6 (`affectedCount` = how many
    /// paths were actually trashed).
    async fn trash_files(
        &self,
        ctx: &Context<'_>,
        paths: Vec<String>,
    ) -> FieldResult<ActionResult> {
        let db = ctx.data::<Arc<Db>>()?;
        // Snapshot the list so we can iterate after `trash_paths` moves them.
        let original: Vec<String> = paths.clone();
        let trashed = trash::trash_paths(paths).await?;
        // Also drop the media index rows: `RemovePath` for files and
        // `purgeMediaIndexByPathPrefix` for directories. Best-effort —
        // failures here don't block the trash operation.
        for raw in &original {
            let _ = scan::delete_by_path(db, raw);
            let _ = scan::delete_by_path_prefix(db, raw);
        }
        Ok(ActionResult {
            affected_count: i32::try_from(trashed.len()).unwrap_or(i32::MAX),
        })
    }

    /// Restore one or more trashed entries by physical trash path or id.
    /// Batch restore → `ActionResult` per API_SPEC §6.
    async fn restore_files(&self, paths: Vec<String>) -> FieldResult<ActionResult> {
        let restored = trash::restore_paths(paths).await?;
        Ok(ActionResult {
            affected_count: i32::try_from(restored.len()).unwrap_or(i32::MAX),
        })
    }

    /// Permanently delete one trashed entry. Path may be a physical trash
    /// path or a trash id. Single idempotent delete → `Boolean!` (§6).
    async fn delete_trash_item(&self, path: String) -> FieldResult<bool> {
        trash::delete_trash_by_path(&path).await?;
        Ok(true)
    }

    /// Create a tag for the given data type and return it.
    async fn create_tag(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        name: String,
    ) -> FieldResult<Tag> {
        let library = ctx.data::<Arc<LibraryDb>>()?;
        let kind = r#type.kind();
        let t = crate::library::tags::create_tag(library, kind, &name)
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(Tag {
            id: t.id.into(),
            name: t.name,
            r#type: t.kind,
            count: t.count,
        })
    }

    /// Rename a tag and return the updated entity.
    async fn update_tag(&self, ctx: &Context<'_>, id: ID, name: String) -> FieldResult<Tag> {
        let library = ctx.data::<Arc<LibraryDb>>()?;
        let t = crate::library::tags::update_tag(library, &id, &name)
            .map_err(|e| async_graphql::Error::new(e.to_string()))?
            .ok_or_else(|| async_graphql::Error::new(format!("tag not found: {}", &*id)))?;
        Ok(Tag {
            id: t.id.into(),
            name: t.name,
            r#type: t.kind,
            count: t.count,
        })
    }

    /// Delete a tag together with its relations. Single idempotent delete → `Boolean!` (§6).
    async fn delete_tag(&self, ctx: &Context<'_>, id: ID) -> FieldResult<bool> {
        let library = ctx.data::<Arc<LibraryDb>>()?;
        crate::library::tags::delete_tag(library, &id);
        Ok(true)
    }

    /// Add a media key to multiple tags. Mirrors plain-app `addToTags`: the
    /// `query` is the shared search DSL (`ids:a,b,c` for checkbox selections,
    /// otherwise the page query) which is resolved to the matching media ids
    /// — one relation per id, keyed by the id (the GraphQL media `id`).
    async fn add_to_tags(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        tag_ids: Vec<ID>,
        query: String,
    ) -> FieldResult<bool> {
        let library = ctx.data::<Arc<LibraryDb>>()?;
        let tag_ids: Vec<String> = tag_ids
            .into_iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        if tag_ids.is_empty() {
            return Ok(true);
        }
        // Fast path: explicit `ids:` selections don't touch the search index.
        let keys = match parse_ids_query(&query) {
            Some(keys) => keys,
            None => resolve_media_keys(&media_index::global(), r#type, &query),
        };
        if keys.is_empty() {
            return Ok(true);
        }
        // plain-app skips ids the tag already has (no duplicate relations).
        let mut add: Vec<(String, String)> = Vec::new();
        for tid in &tag_ids {
            let existing: std::collections::HashSet<String> =
                crate::library::tags::keys_for_tag(library, tid)
                    .into_iter()
                    .collect();
            for k in &keys {
                if !existing.contains(k) {
                    add.push((tid.clone(), k.clone()));
                }
            }
        }
        crate::library::tags::add_relations(library, &add);
        Ok(true)
    }

    /// Per-item tag relation edit. Adds and/or removes tag associations.
    async fn update_tag_relations(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        item: TagRelationStub,
        add_tag_ids: Vec<ID>,
        remove_tag_ids: Vec<ID>,
    ) -> FieldResult<bool> {
        let _ = r#type;
        let library = ctx.data::<Arc<LibraryDb>>()?;
        let mut add: Vec<(String, String)> = Vec::new();
        for tid in &add_tag_ids {
            if tid.is_empty() {
                continue;
            }
            add.push((tid.to_string(), item.key.clone()));
        }
        if !add.is_empty() {
            crate::library::tags::add_relations(library, &add);
        }
        if !remove_tag_ids.is_empty() {
            let remove: Vec<String> = remove_tag_ids.iter().map(|i| i.to_string()).collect();
            crate::library::tags::remove_relations(library, &[item.key.clone()], &remove);
        }
        Ok(true)
    }

    /// Remove the media keys matching `query` from the given tags — the inverse of `addToTags`. Membership op → `Boolean!` (§6).
    async fn remove_from_tags(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        tag_ids: Vec<ID>,
        query: String,
    ) -> FieldResult<bool> {
        let library = ctx.data::<Arc<LibraryDb>>()?;
        let tag_ids: Vec<String> = tag_ids
            .into_iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        if tag_ids.is_empty() {
            return Ok(true);
        }
        // Fast path: explicit `ids:` selections don't touch the search index.
        let keys = match parse_ids_query(&query) {
            Some(keys) => keys,
            None => resolve_media_keys(&media_index::global(), r#type, &query),
        };
        if keys.is_empty() {
            return Ok(true);
        }
        crate::library::tags::remove_relations(library, &keys, &tag_ids);
        Ok(true)
    }

    /// Enqueue a copy task. Returns the FileTask immediately with status=QUEUED;
    /// the worker continues in the background and emits progress via the WS
    /// event `file:task:progress`.
    async fn create_copy_task(
        &self,
        ctx: &Context<'_>,
        ops: Vec<FileTaskOpInput>,
    ) -> FieldResult<FileTask> {
        let cid = ctx.data::<String>().cloned().unwrap_or_default();
        let mapped: Vec<file_tasks::FileTaskOp> = ops
            .into_iter()
            .map(|o| file_tasks::FileTaskOp {
                src: o.src,
                dst: o.dst,
                overwrite: o.overwrite,
            })
            .collect();
        let t = file_tasks::create_copy_task(&cid, mapped)?;
        Ok(task_to_gql(t))
    }

    /// Enqueue a move task. Returns the FileTask immediately with status=QUEUED; progress arrives via the WS event `file:task:progress`.
    async fn create_move_task(
        &self,
        ctx: &Context<'_>,
        ops: Vec<FileTaskOpInput>,
    ) -> FieldResult<FileTask> {
        let cid = ctx.data::<String>().cloned().unwrap_or_default();
        let mapped: Vec<file_tasks::FileTaskOp> = ops
            .into_iter()
            .map(|o| file_tasks::FileTaskOp {
                src: o.src,
                dst: o.dst,
                overwrite: o.overwrite,
            })
            .collect();
        let t = file_tasks::create_move_task(&cid, mapped)?;
        Ok(task_to_gql(t))
    }

    /// Unlike the previous Rust port, we do NOT try to resume a paused
    /// scan here — Go's StartMediaScan always starts a fresh scan. If
    /// the user wants to resume, they call `resumeMediaScan`.
    async fn start_media_scan(&self, ctx: &Context<'_>, root: String) -> FieldResult<bool> {
        let db = ctx.data::<Arc<Db>>()?;
        let source_dirs = ctx
            .data_opt::<Arc<Prefs>>()
            .map(|prefs| kv::media_source::get(prefs))
            .unwrap_or_default();
        let roots = selected_scan_roots(root.clone(), &source_dirs);
        scan::start_walk_and_scan_paths(db.clone(), roots, std::path::PathBuf::from(root)).await?;
        Ok(true)
    }

    /// Pause the running media scan; publishes a progress event immediately so the UI flips to `PAUSED` without waiting for the next ticker tick.
    async fn pause_media_scan(&self) -> FieldResult<bool> {
        scan::scanner().pause();
        // Mirror Go `PauseScan`: push a progress event so the UI flips
        // to "paused" immediately instead of waiting for the next 1s
        // ticker tick.
        scan::publish_state_event();
        Ok(true)
    }

    /// Resume a paused media scan; publishes a progress event immediately.
    async fn resume_media_scan(&self) -> FieldResult<bool> {
        scan::scanner().resume();
        // Same as pause: tell the UI the new state right away.
        scan::publish_state_event();
        Ok(true)
    }

    /// Stop the media scan and publish a final `STOPPED` progress event (without `root`). A later `startMediaScan` begins a fresh scan.
    async fn stop_media_scan(&self) -> FieldResult<bool> {
        let s = scan::scanner();
        s.stop();
        scan::publish_stopped_event();
        Ok(true)
    }

    /// Rebuild the media index from scratch. Mirrors Go `rebuildMediaIndex`
    /// in `internal/graph/media_scan_api.go`:
    /// 1. `media.StopScan()` — set the stop flag so the running scan loop
    ///    exits at its next yield.
    /// 2. `media.ResetAllMediaData()` — wipe every media index row.
    /// 3. `media.ResumeScan()` — clear the stop flag.
    /// 4. Synchronously publish the initial `{state:"running", root:...}`
    ///    event so the UI can show the spinner **before** we go off-thread
    ///    to do the heavy reset + walk. (Go does this in the same call
    ///    stack as the GraphQL resolver; the WS hub handler then receives
    ///    the event via the in-process eventbus.)
    /// 5. `go media.ScanAndSync(root)` — fire the actual walk+index on a
    ///    background tokio task. The resolver returns `true` immediately.
    /// The reset and the walk are moved off the API thread so a large
    /// library does not block other GraphQL calls; this is a deliberate
    /// divergence from the Go side, where `ResetAllMediaData` runs on the
    /// resolver goroutine. Functionally equivalent.
    async fn rebuild_media_index(&self, ctx: &Context<'_>, root: String) -> FieldResult<bool> {
        let db = ctx.data::<Arc<Db>>()?.clone();
        let source_dirs = ctx
            .data_opt::<Arc<Prefs>>()
            .map(|prefs| kv::media_source::get(prefs))
            .unwrap_or_default();
        let root_path = std::path::PathBuf::from(&root);
        let roots = selected_scan_roots(root, &source_dirs);

        let s = scan::scanner();
        // 1) signal any in-flight scan to stop; we don't wait for it here
        //    because the stop flag is checked cooperatively in the loop.
        s.stop();
        // 3) clear the stop flag so the upcoming scan can run.
        s.resume();
        // 4) publish the initial "running" event synchronously so the UI
        //    sees the scan start before we yield the API task.
        scan::publish_initial_running(&root_path);

        // 2 + 5: heavy work goes off-thread. `reset_all` does synchronous
        // KV IO and would block the tokio worker for the full duration of
        // the wipe (and the subsequent walk), so it runs on the blocking
        // thread pool.
        let db_for_reset = db.clone();
        let root_for_walk = root_path.clone();
        tokio::spawn(async move {
            log::info!("[scan] rebuild tokio::spawn body entered");
            // Make sure the previous scan is fully gone before we reset
            // (so the abort doesn't fight with the delete loop).
            scan::scanner().abort_running_task().await;
            let db_for_reset2 = db_for_reset.clone();
            let reset_res = tokio::task::spawn_blocking(move || {
                log::info!("[scan] reset_all blocking body entered");
                let r = scan::reset_all(&db_for_reset2);
                log::info!("[scan] reset_all blocking body returned");
                r
            })
            .await;
            log::info!("[scan] rebuild reset_all joined, ok={}", reset_res.is_ok());
            match reset_res {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    log::error!("rebuild_media_index reset_all failed: {e}");
                }
                Err(e) => {
                    log::error!("rebuild_media_index reset_all join error: {e}");
                }
            }
            // Even if reset fails we still try to start a fresh scan so
            // the index eventually converges.
            if let Err(e) =
                scan::start_walk_and_scan_paths(db_for_reset, roots, root_for_walk).await
            {
                log::error!("rebuild_media_index background scan failed: {e}");
            }
        });
        Ok(true)
    }

    /// Persist the list of source directories the media scanner should
    /// index.
    async fn set_media_source_dirs(
        &self,
        ctx: &Context<'_>,
        dirs: Vec<String>,
    ) -> FieldResult<bool> {
        let prefs = ctx.data::<Arc<Prefs>>()?;
        kv::media_source::set(prefs, &dirs)?;
        Ok(true)
    }

    /// Move the media items matching `query` into their disks' `.nas-trash`; `affectedCount` = items trashed. Blank query is rejected (`bulk_query_required`, §5).
    async fn trash_media_items(
        &self,
        ctx: &Context<'_>,
        #[graphql(name = "type")] media_type: MediaDataType,
        query: String,
    ) -> FieldResult<ActionResult> {
        run_media_items_action(ctx, media_type, &query, MediaItemsAction::Trash).await
    }

    /// Restore trashed media items matching `query` (implies `trash:true`); `affectedCount` = items restored. Blank query is rejected (`bulk_query_required`, §5).
    async fn restore_media_items(
        &self,
        ctx: &Context<'_>,
        #[graphql(name = "type")] media_type: MediaDataType,
        query: String,
    ) -> FieldResult<ActionResult> {
        run_media_items_action(ctx, media_type, &query, MediaItemsAction::Restore).await
    }

    /// Permanently delete media items matching `query` (physical delete + index row removal); `affectedCount` = items deleted. Blank query is rejected (`bulk_query_required`, §5).
    async fn delete_media_items(
        &self,
        ctx: &Context<'_>,
        #[graphql(name = "type")] media_type: MediaDataType,
        query: String,
    ) -> FieldResult<ActionResult> {
        run_media_items_action(ctx, media_type, &query, MediaItemsAction::Delete).await
    }

    /// Move the selected media files into `destDir` (plain-app
    /// `moveMediaItems`): filesystem move + index/db rewrite of the new path.
    async fn move_media_items(
        &self,
        ctx: &Context<'_>,
        #[graphql(name = "type")] media_type: MediaDataType,
        query: String,
        #[graphql(name = "destDir")] dest_dir: String,
    ) -> FieldResult<ActionResult> {
        let dest = std::path::PathBuf::from(&dest_dir);
        if !dest.is_dir() {
            return Err(async_graphql::Error::new(format!(
                "dest_dir is not a directory: {dest_dir}"
            )));
        }
        let db = ctx.data::<Arc<Db>>()?.clone();
        let library = (**ctx.data::<Arc<LibraryDb>>()?).clone();
        let (ids, text) = media_bulk_selection(&query)?;
        let uuids: Vec<String> = if !ids.trim().is_empty() {
            ids.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        } else {
            match media_index::global().search(
                &text,
                media_type.data_type().media_type_str(),
                None,
                MediaSort::DateDesc,
                0,
                10_000,
            ) {
                Ok(rows) => rows.into_iter().map(|r| r.uuid).collect(),
                Err(e) => {
                    log::error!("[media-items] move search failed: {e}");
                    Vec::new()
                }
            }
        };

        let idx = media_index::global();
        let mut moved = 0i32;
        for uuid in &uuids {
            let m = match scan::get_by_uuid(&db, uuid) {
                Ok(Some(m)) => m,
                _ => continue,
            };
            let new_path = dest.join(file_name_of(&m.path));
            let renamed = std::fs::rename(&m.path, &new_path).is_ok()
                || (std::fs::copy(&m.path, &new_path).is_ok()
                    && std::fs::remove_file(&m.path).is_ok());
            if !renamed {
                log::debug!(
                    "[media-items] move {} -> {}: failed",
                    m.path,
                    new_path.display()
                );
                continue;
            }
            let mut updated = m.clone();
            updated.path = new_path.to_string_lossy().into_owned();
            if scan::upsert_media_row(&db, &updated).is_err() {
                continue;
            }
            let _ = idx.remove_by_uuid(uuid);
            let _ = idx.add_media_file(&updated);
            // Stale queue entries pointed at the old path are dropped, the
            // same way trash prunes them.
            if updated.r#type == "audio" {
                let _ = crate::library::audio_queue::remove_paths(
                    &library,
                    [m.path.clone()].as_slice(),
                );
            }
            moved += 1;
        }
        let _ = idx.commit();
        Ok(ActionResult {
            affected_count: moved,
        })
    }
}

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

fn tags_by_key(library: &LibraryDb, key: &str, kind: i32) -> Vec<Tag> {
    crate::library::tags::tags_for_key_of_kind(library, key, kind)
        .into_iter()
        .map(|t| Tag {
            id: t.id.into(),
            name: t.name,
            r#type: t.kind,
            count: t.count,
        })
        .collect()
}

fn lazy_tags(ctx: &Context<'_>, id: &str, data_type: DataType) -> Vec<Tag> {
    count_tag_load();
    ctx.data::<Arc<LibraryDb>>()
        .ok()
        .map(|l| tags_by_key(l, id, data_type.kind()))
        .unwrap_or_default()
}

#[async_graphql::ComplexObject]
impl Audio {
    /// Tags resolve lazily — only when the selection asks for the field,
    /// mirroring plain-app's DataLoader `dataProperty`.
    pub async fn tags(&self, ctx: &Context<'_>) -> Vec<Tag> {
        lazy_tags(ctx, &self.id, DataType::Audio)
    }
}

#[async_graphql::ComplexObject]
impl Image {
    pub async fn tags(&self, ctx: &Context<'_>) -> Vec<Tag> {
        lazy_tags(ctx, &self.id, DataType::Image)
    }
}

#[async_graphql::ComplexObject]
impl Video {
    pub async fn tags(&self, ctx: &Context<'_>) -> Vec<Tag> {
        lazy_tags(ctx, &self.id, DataType::Video)
    }
}

#[async_graphql::ComplexObject]
impl Doc {
    pub async fn tags(&self, ctx: &Context<'_>) -> Vec<Tag> {
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

/// Media-index count for one media type under the app search DSL.
fn media_count(query: &str, media_type: Option<&str>) -> i32 {
    match media_index::global().count(query, media_type, None) {
        Ok(n) => n.min(i32::MAX as usize) as i32,
        Err(e) => {
            log::error!("[gql] media count failed: {e}");
            0
        }
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

/// Resolve the directory a `files` query lists: `parent` (DSL) overrides
/// `root` (plain-app argument), which overrides `root_path` (legacy Go DSL
/// field); `relative_path` (legacy Go DSL) joins underneath. Empty input
/// lists `/`.
fn files_base_dir(parent: &str, root: &str, root_path: &str, relative_path: &str) -> String {
    let base = if !parent.is_empty() {
        parent
    } else if !root.is_empty() {
        root
    } else {
        root_path
    };
    if relative_path.is_empty() {
        if base.is_empty() {
            "/".to_string()
        } else {
            base.to_string()
        }
    } else {
        format!(
            "{}/{}",
            if base.is_empty() {
                "/"
            } else {
                base.trim_end_matches('/')
            },
            relative_path.trim_start_matches('/')
        )
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

/// The explicit `ids:a,b,c` DSL group from a tag-mutation query (checkbox
/// selections) → the media keys to tag. `None` when the query has no ids.
fn parse_ids_query(query: &str) -> Option<Vec<String>> {
    let fields = crate::utils::search_dsl::parse(query);
    let mut ids = String::new();
    for f in &fields {
        if f.name == "ids" {
            ids = f.value.clone();
        }
    }
    if ids.trim().is_empty() {
        return None;
    }
    Some(
        ids.split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

/// Resolve a tag-mutation `query` (the shared search DSL: `ids:a,b,c` for
/// checkbox selections, otherwise the page query) into the media keys to
/// tag — the media row uuids, which are also the GraphQL media `id`s used
/// as tag relation keys. Mirrors plain-app `getMediaIds(type, query)`.
fn resolve_media_keys(
    index: &media_index::MediaSearchIndex,
    media_type: DataType,
    query: &str,
) -> Vec<String> {
    if let Some(keys) = parse_ids_query(query) {
        return keys;
    }
    // Pass the full page query through so list filters (trash:, parent:, …)
    // scope the tagging exactly like plain-app's MediaStore search.
    match index.search(
        query,
        media_type.media_type_str(),
        None,
        MediaSort::DateDesc,
        0,
        10_000,
    ) {
        Ok(rows) => rows.into_iter().map(|r| r.uuid).collect(),
        Err(e) => {
            log::error!("[tags] resolve keys failed for {query:?}: {e}");
            Vec::new()
        }
    }
}

// ----- Media items trash / restore / delete -----

#[derive(Clone, Copy, PartialEq, Debug)]
enum MediaItemsAction {
    Trash,
    Restore,
    Delete,
}

/// Shared selection for the query-addressed media bulk ops
/// (delete/trash/restore/move). Blank queries are rejected — whole-table
/// intent must be the explicit `all:true` sentinel (API_SPEC §5): parsing
/// `all:true` yields a named field that passes this guard and is ignored
/// by the selection below, degrading to the unfiltered search exactly
/// like the phone's `all`-ignoring where-builder.
fn media_bulk_selection(query: &str) -> FieldResult<(String, String)> {
    let fields = crate::utils::search_dsl::parse(query);
    if fields.is_empty() {
        return Err(async_graphql::Error::new("bulk_query_required"));
    }
    let mut ids = String::new();
    let mut text = String::new();
    for f in &fields {
        if f.name == "ids" {
            ids = f.value.clone();
        } else if f.name == "text" {
            text = f.value.clone();
        }
    }
    Ok((ids, text))
}

/// Shared engine for `trashMediaItems` / `restoreMediaItems` /
/// `deleteMediaItems`. The query DSL may carry `ids:a,b,c` (operate on those
/// uuids) and/or free `text`; without ids the matching media rows (up to
/// 10 000) are resolved from the media search index — with `trash:true`
/// forced in for restore/delete, since only trashed items can be
/// restored or permanently deleted.
async fn run_media_items_action(
    ctx: &Context<'_>,
    media_type: MediaDataType,
    query: &str,
    action: MediaItemsAction,
) -> FieldResult<ActionResult> {
    let db = ctx.data::<Arc<Db>>()?.clone();
    let library = (**ctx.data::<Arc<LibraryDb>>()?).clone();
    let (ids, text) = media_bulk_selection(query)?;

    let type_str = media_type.data_type().media_type_str();

    let uuids: Vec<String> = if !ids.trim().is_empty() {
        ids.split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    } else {
        let trash_filter = match action {
            MediaItemsAction::Restore | MediaItemsAction::Delete => Some(true),
            MediaItemsAction::Trash => None,
        };
        match media_index::global().search(
            &text,
            type_str,
            trash_filter,
            MediaSort::DateDesc,
            0,
            10_000,
        ) {
            Ok(rows) => rows.into_iter().map(|r| r.uuid).collect(),
            Err(e) => {
                log::error!("[media-items] search failed: {e}");
                Vec::new()
            }
        }
    };

    for uuid in &uuids {
        let m = match scan::get_by_uuid(&db, uuid) {
            Ok(Some(m)) => m,
            _ => continue,
        };
        if let Err(e) = apply_media_items_action(&db, &library, &m, action).await {
            log::debug!("[media-items] {action:?} {}: {e}", m.path);
        }
    }

    Ok(ActionResult {
        affected_count: uuids.len() as i32,
    })
}

/// Apply one action to one media row (mirrors Go `media.TrashUUID` /
/// `RestoreUUID` / `DeleteUUIDPermanently`).
async fn apply_media_items_action(
    db: &Arc<Db>,
    library: &LibraryDb,
    m: &scan::MediaFile,
    action: MediaItemsAction,
) -> anyhow::Result<()> {
    match action {
        MediaItemsAction::Trash => {
            if m.is_trash {
                return Ok(());
            }
            // plain-app prunes trashed audios out of the playback queue,
            // playlists and history (`AudioQueueManager.removePaths`).
            if m.r#type == "audio" {
                crate::library::audio_queue::remove_paths(library, std::slice::from_ref(&m.path));
            }
            let trashed = trash::trash_paths(vec![m.path.clone()]).await?;
            let mut updated = m.clone();
            updated.path = trashed
                .first()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("trash failed"))?;
            updated.trash_path = updated.path.clone();
            if updated.original_path.is_empty() {
                updated.original_path = m.path.clone();
            }
            updated.is_trash = true;
            updated.deleted_at = chrono::Utc::now().timestamp();
            // Tag relations hang off the media identity; a trashed item is
            // out of the library until restored.
            for key in [m.uuid.clone(), m.path.clone()] {
                crate::library::tags::remove_relations_for_keys(library, &[key]);
            }
            scan::upsert_media_row(db, &updated)?;
        }
        MediaItemsAction::Restore => {
            if !m.is_trash {
                return Ok(());
            }
            let restored = trash::restore_paths(vec![m.path.clone()]).await?;
            let mut updated = m.clone();
            updated.path = restored
                .first()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("restore failed"))?;
            updated.is_trash = false;
            updated.trash_path = String::new();
            updated.deleted_at = 0;
            scan::upsert_media_row(db, &updated)?;
        }
        MediaItemsAction::Delete => {
            if m.r#type == "audio" {
                crate::library::audio_queue::remove_paths(library, std::slice::from_ref(&m.path));
            }
            if trash::is_trashed_path(&m.path) {
                trash::delete_trash_by_path(&m.path).await?;
            } else {
                let p = std::path::Path::new(&m.path);
                if p.is_file() {
                    std::fs::remove_file(p)?;
                } else if p.is_dir() {
                    std::fs::remove_dir_all(p)?;
                }
            }
            scan::delete_by_uuid(db, &m.uuid)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/unit/media/gql.rs"]
mod tests;
