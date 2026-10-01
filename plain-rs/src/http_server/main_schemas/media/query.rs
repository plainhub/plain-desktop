use super::*;

// ---------------------------------------------------------------------------
// Query root
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct MediaQueryRoot;

#[Object]
impl MediaQueryRoot {
    /// Count of entries in a directory (non-recursive).
    async fn file_count(&self, root: String, query: String) -> FieldResult<i32> {
        Ok(crate::media::file_browse::count_files(&root, &query)?)
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

    /// File listing with directory, search, or trash results.
    async fn files(
        &self,
        root: String,
        offset: i32,
        limit: i32,
        query: String,
        sort_by: FileSortBy,
    ) -> FieldResult<Vec<File>> {
        let sort = match sort_by {
            FileSortBy::NAME_ASC => fsx::SortBy::NameAsc,
            FileSortBy::NAME_DESC => fsx::SortBy::NameDesc,
            FileSortBy::DATE_ASC => fsx::SortBy::DateAsc,
            FileSortBy::DATE_DESC | FileSortBy::TAKEN_AT_DESC => fsx::SortBy::DateDesc,
            FileSortBy::SIZE_ASC => fsx::SortBy::SizeAsc,
            FileSortBy::SIZE_DESC => fsx::SortBy::SizeDesc,
        };
        match crate::media::file_browse::list_files(&root, offset, limit, &query, sort).await? {
            crate::media::file_browse::FilesPage::Entries(entries) => {
                Ok(entries.into_iter().map(file_entry_to_gql).collect())
            }
            crate::media::file_browse::FilesPage::Trash(items) => {
                Ok(items.into_iter().map(trash_item_to_file).collect())
            }
        }
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
        let library = ctx.data::<Arc<SqlDb>>()?;
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
        let library = ctx.data::<Arc<SqlDb>>()?;
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
        let rows = crate::media::catalog::search_page(
            &query,
            Some("audio"),
            media_sort(sort_by),
            offset,
            limit,
            ctx.data::<Arc<Db>>().ok().cloned(),
            true,
        )
        .await?;
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
        Ok(crate::media::catalog::count(&query, Some("audio")))
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
        let rows = crate::media::catalog::search_page(
            &query,
            Some("image"),
            media_sort(sort_by),
            offset,
            limit,
            None,
            false,
        )
        .await?;
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
        Ok(crate::media::catalog::count(&query, Some("image")))
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
        let rows = crate::media::catalog::search_page(
            &query,
            Some("video"),
            media_sort(sort_by),
            offset,
            limit,
            ctx.data::<Arc<Db>>().ok().cloned(),
            false,
        )
        .await?;
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
        Ok(crate::media::catalog::count(&query, Some("video")))
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
        let rows = crate::media::catalog::search_page(
            &query,
            Some("doc"),
            media_sort(sort_by),
            offset,
            limit,
            None,
            false,
        )
        .await?;
        Ok(rows.into_iter().map(doc_to_gql).collect())
    }

    /// Document count (tantivy counting collector).
    async fn doc_count(&self, query: String) -> FieldResult<i32> {
        Ok(crate::media::catalog::count(&query, Some("doc")))
    }

    /// Per-extension group counts for the docs sidebar (no query filter,
    /// plain-app semantics — includes trashed, matching `docCount("")`).
    async fn doc_ext_groups(&self) -> FieldResult<Vec<DocExtGroup>> {
        let groups = crate::media::catalog::doc_ext_groups().await?;
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
        let buckets = crate::media::catalog::buckets(db, kind).await?;
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
