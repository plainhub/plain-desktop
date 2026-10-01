use super::*;

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
        Ok(file_entry_to_gql(
            crate::media::file_ops::create_dir(&path).await?,
        ))
    }

    /// Write a text file. Mirrors Go `writeTextFileModel` (2 MiB cap,
    /// overwrite-or-error on existing). Returns the resulting `File!`.
    async fn write_text_file(
        &self,
        path: String,
        content: String,
        overwrite: bool,
    ) -> FieldResult<File> {
        Ok(file_entry_to_gql(
            crate::media::file_ops::write_text_file(&path, &content, overwrite).await?,
        ))
    }

    /// Rename a file or directory in place: `path` is the current full path, `name` the new base name. Errors (missing target, name clash) surface as GraphQL errors.
    async fn rename_file(&self, path: String, name: String) -> FieldResult<bool> {
        crate::media::file_ops::rename_file(&path, &name).await?;
        Ok(true)
    }

    /// Synchronous single-file copy (path-addressed). `overwrite: false` fails when `dst` exists; for batches or large trees use `createCopyTask`.
    async fn copy_file(&self, src: String, dst: String, overwrite: bool) -> FieldResult<bool> {
        crate::media::file_ops::copy_file(&src, &dst, overwrite).await?;
        Ok(true)
    }

    /// Synchronous single-file move (path-addressed). `overwrite: false` fails when `dst` exists; for batches or large trees use `createMoveTask`.
    async fn move_file(&self, src: String, dst: String, overwrite: bool) -> FieldResult<bool> {
        crate::media::file_ops::move_file(&src, &dst, overwrite).await?;
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
        let affected = crate::media::file_ops::delete_files(db, &paths).await?;
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
        let affected = crate::media::file_ops::trash_files(db, paths).await?;
        Ok(ActionResult {
            affected_count: affected,
        })
    }

    /// Restore one or more trashed entries by physical trash path or id.
    /// Batch restore → `ActionResult` per API_SPEC §6.
    async fn restore_files(&self, paths: Vec<String>) -> FieldResult<ActionResult> {
        let affected = crate::media::file_ops::restore_files(paths).await?;
        Ok(ActionResult {
            affected_count: affected,
        })
    }

    /// Permanently delete one trashed entry. Path may be a physical trash
    /// path or a trash id. Single idempotent delete → `Boolean!` (§6).
    async fn delete_trashed_file(&self, path: String) -> FieldResult<bool> {
        crate::media::file_ops::delete_trash_item(&path).await?;
        Ok(true)
    }

    /// Create a tag for the given data type and return it.
    async fn create_tag(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        name: String,
    ) -> FieldResult<Tag> {
        let library = ctx.data::<Arc<SqlDb>>()?;
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
        let library = ctx.data::<Arc<SqlDb>>()?;
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
        let library = ctx.data::<Arc<SqlDb>>()?;
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
        let library = ctx.data::<Arc<SqlDb>>()?;
        let tag_ids: Vec<String> = tag_ids
            .into_iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        crate::media::tagging::add_to_tags(library, r#type, &tag_ids, &query);
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
        let library = ctx.data::<Arc<SqlDb>>()?;
        let _ = r#type;
        let add = add_tag_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>();
        let remove = remove_tag_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>();
        crate::media::tagging::update_relations(library, &item.key, &add, &remove);
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
        let library = ctx.data::<Arc<SqlDb>>()?;
        let tag_ids: Vec<String> = tag_ids
            .into_iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        crate::media::tagging::remove_from_tags(library, r#type, &tag_ids, &query);
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
        let roots = scan::selected_roots(root.clone(), &source_dirs);
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

    /// Rebuild the media index from scratch and begin a fresh scan.
    async fn rebuild_media_index(&self, ctx: &Context<'_>, root: String) -> FieldResult<bool> {
        let db = ctx.data::<Arc<Db>>()?.clone();
        let source_dirs = ctx
            .data_opt::<Arc<Prefs>>()
            .map(|prefs| kv::media_source::get(prefs))
            .unwrap_or_default();
        scan::rebuild_index(db, root, &source_dirs).await;
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
        run_media_items_action(
            ctx,
            media_type,
            &query,
            crate::media::item_ops::MediaItemsAction::Trash,
        )
        .await
    }

    /// Restore trashed media items matching `query` (implies `trash:true`); `affectedCount` = items restored. Blank query is rejected (`bulk_query_required`, §5).
    async fn restore_media_items(
        &self,
        ctx: &Context<'_>,
        #[graphql(name = "type")] media_type: MediaDataType,
        query: String,
    ) -> FieldResult<ActionResult> {
        run_media_items_action(
            ctx,
            media_type,
            &query,
            crate::media::item_ops::MediaItemsAction::Restore,
        )
        .await
    }

    /// Permanently delete media items matching `query` (physical delete + index row removal); `affectedCount` = items deleted. Blank query is rejected (`bulk_query_required`, §5).
    async fn delete_media_items(
        &self,
        ctx: &Context<'_>,
        #[graphql(name = "type")] media_type: MediaDataType,
        query: String,
    ) -> FieldResult<ActionResult> {
        run_media_items_action(
            ctx,
            media_type,
            &query,
            crate::media::item_ops::MediaItemsAction::Delete,
        )
        .await
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
        let db = ctx.data::<Arc<Db>>()?.clone();
        let library = (**ctx.data::<Arc<SqlDb>>()?).clone();
        let moved = crate::media::item_ops::move_media_items(
            &db,
            &library,
            media_type.data_type().media_type_str(),
            &query,
            &dest_dir,
        )
        .await?;
        Ok(ActionResult {
            affected_count: moved,
        })
    }
}

async fn run_media_items_action(
    ctx: &Context<'_>,
    media_type: MediaDataType,
    query: &str,
    action: crate::media::item_ops::MediaItemsAction,
) -> FieldResult<ActionResult> {
    let db = ctx.data::<Arc<Db>>()?.clone();
    let library = (**ctx.data::<Arc<SqlDb>>()?).clone();
    let affected_count = crate::media::item_ops::run_media_items_action(
        &db,
        &library,
        media_type.data_type().media_type_str(),
        query,
        action,
    )
    .await?;
    Ok(ActionResult { affected_count })
}
