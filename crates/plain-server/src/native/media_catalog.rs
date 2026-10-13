use crate::assembly::server::ServerState;
use plain_rs::db::{MediaFileMetadata, MediaFileRow};
use plain_rs::enums::DataType;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CountRequest {
    data_type: DataType,
    query: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct IdsRequest {
    data_type: DataType,
    query: String,
    from_trash: bool,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RowsRequest {
    data_type: DataType,
    query: String,
    sort_by: crate::graphql::resolvers::files::types::FileSortBy,
    limit: u32,
    offset: u32,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BucketRequest {
    data_type: DataType,
}

#[cfg(feature = "media")]
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CompletedFilesRequest {
    paths: Vec<String>,
}

#[cfg(feature = "media")]
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeleteActionRequest {
    #[serde(rename = "type")]
    data_type: i32,
    action: plain_rs::media::actions::Action,
    ids: Vec<String>,
    from_trash: bool,
    dest_dir: String,
}

#[derive(Clone)]
pub struct NativeMediaCatalog {
    state: ServerState,
}

impl NativeMediaCatalog {
    pub(crate) fn new(state: ServerState) -> Self {
        Self { state }
    }

    pub fn source_directories(&self) -> Result<Vec<String>, String> {
        self.state.native_client_id()?;
        plain_rs::prefs::media::source_directories(&self.state.prefs)
            .map_err(|error| error.to_string())
    }

    pub fn set_source_directories(&self, directories: &[String]) -> Result<(), String> {
        self.state.native_client_id()?;
        plain_rs::prefs::media::set_source_directories(&self.state.prefs, directories)
            .map_err(|error| error.to_string())
    }

    #[cfg(feature = "media")]
    pub async fn scan_file(
        &self,
        data_type: DataType,
        reference: &str,
    ) -> Result<MediaFileRow, String> {
        let metadata = self.read_file(data_type, reference).await?;
        self.record_file_reads(&[metadata])?
            .into_iter()
            .next()
            .ok_or_else(|| "Missing recorded media file".into())
    }

    #[cfg(feature = "media")]
    async fn read_file(
        &self,
        data_type: DataType,
        reference: &str,
    ) -> Result<plain_rs::db::MediaFileMetadataRead, String> {
        let path =
            crate::services::files::file_access::resolve_native(&self.state, reference).await?;
        let metadata = tokio::task::spawn_blocking(move || {
            plain_media::metadata::file_facts::read_file_metadata_with_availability(
                &path, data_type,
            )
            .map_err(|error| error.to_string())
        })
        .await
        .map_err(|error| error.to_string())??;
        Ok(metadata)
    }

    #[cfg(feature = "media")]
    pub async fn scan_files(
        &self,
        data_type: DataType,
        references: Vec<String>,
    ) -> Result<crate::services::files::media_scans::Snapshot, String> {
        self.state.native_client_id()?;
        data_type
            .media_type_str()
            .ok_or("Unsupported media catalog type")?;
        let session = self.state.media_scans.begin(references.len())?;
        let mut cancel = session.cancel.clone();
        let mut stop = self.state.stop.clone();
        let result = async {
            for reference in references {
                session.wait_running().await?;
                let metadata = tokio::select! {
                    biased;
                    _ = crate::services::files::media_scans::wait_stop(&mut cancel) => return Err("Media scan cancelled".into()),
                    _ = crate::services::files::media_scans::wait_stop(&mut stop) => {
                        self.state.media_scans.cancel()?;
                        return Err("Rust core stopped".into());
                    },
                    metadata = self.read_file(data_type, &reference) => metadata?,
                };
                session.record_when_running(|| self.record_file_reads(&[metadata])).await?;
            }
            Ok(())
        }.await;
        session.finish(result)
    }

    #[cfg(feature = "media")]
    pub fn scan_completed_files(&self, params: serde_json::Value) -> Result<bool, String> {
        self.state.native_client_id()?;
        let request: CompletedFilesRequest =
            serde_json::from_value(params).map_err(|error| error.to_string())?;
        let mut files = Vec::new();
        if !request.paths.is_empty() {
            crate::host::file_resource::authorize_read(
                serde_json::json!({"operation":"read","paths":request.paths}),
            )?;
            let policy = self.state.media_scans.policy()?;
            let directory = self
                .state
                .directory
                .canonicalize()
                .map_err(|error| error.to_string())?;
            for path in request.paths {
                let path = std::path::Path::new(&path)
                    .canonicalize()
                    .map_err(|error| error.to_string())?;
                if !path.is_file() {
                    return Err("Regular completed file required".into());
                }
                let name = path
                    .to_str()
                    .ok_or("Invalid completed file path encoding")?;
                if plain_media::scan::is_media_excluded_at(
                    name,
                    &directory,
                    &policy.cache_directory,
                    &policy.excluded_roots,
                ) {
                    continue;
                }
                let kind = plain_media::scan::infer_type(name);
                let Some(data_type) = [
                    DataType::Audio,
                    DataType::Video,
                    DataType::Image,
                    DataType::Doc,
                ]
                .into_iter()
                .find(|data_type| data_type.media_type_str() == Some(kind)) else {
                    continue;
                };
                let read = plain_media::metadata::file_facts::read_file_metadata_with_availability(
                    &path, data_type,
                )
                .map_err(|error| error.to_string())?;
                if data_type == DataType::Doc && read.metadata.size_bytes == 0 {
                    continue;
                }
                files.push(read);
            }
        }
        self.state
            .media_scans
            .record_completion(|| self.record_file_reads(&files))?;
        Ok(true)
    }

    #[cfg(feature = "media")]
    pub fn configure_scan_directories(
        &self,
        cache_directory: &std::path::Path,
        config_file: Option<&std::path::Path>,
    ) -> Result<(), String> {
        self.state.native_client_id()?;
        self.state
            .media_scans
            .configure(crate::services::files::media_scan_policy::Policy::load(
                cache_directory,
                config_file,
            )?)
    }

    #[cfg(feature = "media")]
    pub async fn scan_directory(
        &self,
        reference: &str,
    ) -> Result<crate::services::files::media_scans::Snapshot, String> {
        self.scan_directories(vec![reference.to_owned()]).await
    }

    /// Watch the configured source roots so a file that appears, changes or
    /// leaves keeps the catalog current without a full rescan.
    ///
    /// The watcher is held by the core; `stop_core` releases it. Re-watching
    /// the same roots replaces the previous watch rather than stacking a
    /// second one.
    #[cfg(feature = "media")]
    pub fn watch_source_directories(&self) -> Result<bool, String> {
        self.state.native_client_id()?;
        let policy = self.state.media_scans.policy()?;
        let directory = self
            .state
            .directory
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let roots = self
            .source_directories()?
            .into_iter()
            .map(std::path::PathBuf::from)
            .filter(|root| root.is_dir())
            .collect::<Vec<_>>();
        if roots.is_empty() {
            self.state
                .media_watch
                .lock()
                .map_err(|_| "Media watcher unavailable")?
                .take();
            return Ok(false);
        }
        let runtime = tokio::runtime::Handle::try_current().map_err(|error| error.to_string())?;
        let state = self.state.clone();
        let catalog_state = self.state.clone();
        let handle = crate::services::files::media_watch::start(&roots, move |path, kind| {
            // `notify` delivers on its own thread, so the work moves onto the
            // runtime rather than blocking the watcher.
            let state = state.clone();
            let path = path.to_path_buf();
            let kind = kind.clone();
            let cache = policy.cache_directory.clone();
            let excluded_roots = policy.excluded_roots.clone();
            // The closure is `Fn`, so each delivery takes its own copies.
            let directory = directory.clone();
            let catalog_state = catalog_state.clone();
            runtime.spawn(async move {
                let excluded = plain_media::scan::is_media_excluded_at(
                    &path.to_string_lossy(),
                    &directory,
                    &cache,
                    &excluded_roots,
                );
                let effect =
                    crate::services::files::media_watch::effect_for(&path, &kind, excluded);
                let catalog = crate::native::media_catalog::NativeMediaCatalog::new(catalog_state);
                match effect {
                    crate::services::files::media_watch::Effect::Index => {
                        if let Err(error) = catalog.scan_completed_files(
                            serde_json::json!({"paths":[path.to_string_lossy()]}),
                        ) {
                            log::debug!("media watch index failed: {error}");
                        }
                    }
                    crate::services::files::media_watch::Effect::Forget => {
                        let name = path.to_string_lossy().into_owned();
                        if let Err(error) = state.db.forget_media_file(&name) {
                            log::debug!("media watch forget failed: {error}");
                        }
                    }
                    crate::services::files::media_watch::Effect::RescanSubtree => {
                        if let Err(error) = catalog.scan_directory(&path.to_string_lossy()).await {
                            log::debug!("media watch rescan failed: {error}");
                        }
                    }
                    crate::services::files::media_watch::Effect::Ignore => {}
                }
            });
        })
        .map_err(|error| error.to_string())?;
        let mut slot = self
            .state
            .media_watch
            .lock()
            .map_err(|_| "media watch unavailable")?;
        if slot.is_some() {
            // Replacing drops the previous watcher, which stops the old one.
            *slot = None;
        }
        *slot = Some(handle);
        Ok(true)
    }

    #[cfg(feature = "media")]
    pub async fn scan_source_directories(
        &self,
        root: &str,
    ) -> Result<crate::services::files::media_scans::Snapshot, String> {
        self.scan_directories(self.source_references(root)?).await
    }

    #[cfg(feature = "media")]
    fn source_references(&self, root: &str) -> Result<Vec<String>, String> {
        let sources = self.source_directories()?;
        plain_media::scan::selected_roots(root.to_owned(), &sources)
            .into_iter()
            .map(|path| {
                path.into_os_string()
                    .into_string()
                    .map_err(|_| "Invalid media directory path encoding".to_string())
            })
            .collect::<Result<Vec<_>, _>>()
    }

    #[cfg(feature = "media")]
    pub fn start_source_scan(
        &self,
        root: &str,
    ) -> Result<crate::services::files::media_scans::Snapshot, String> {
        let runtime = tokio::runtime::Handle::try_current().map_err(|error| error.to_string())?;
        let references = self.source_references(root)?;
        let policy = self.state.media_scans.policy()?;
        let session = self.state.media_scans.begin_discovery()?;
        let catalog = self.clone();
        self.state
            .media_scans
            .spawn(&runtime, session, move |session| {
                Box::pin(async move {
                    if let Err(error) = catalog
                        .scan_directory_session(references, policy, session)
                        .await
                    {
                        log::error!("Media scan task failed: {error}");
                    }
                })
            })
    }

    #[cfg(feature = "media")]
    pub async fn scan_directories(
        &self,
        references: Vec<String>,
    ) -> Result<crate::services::files::media_scans::Snapshot, String> {
        self.state.native_client_id()?;
        let policy = self.state.media_scans.policy()?;
        let session = self.state.media_scans.begin_discovery()?;
        self.scan_directory_session(references, policy, session)
            .await
    }

    #[cfg(feature = "media")]
    async fn scan_directory_session(
        &self,
        references: Vec<String>,
        policy: std::sync::Arc<crate::services::files::media_scan_policy::Policy>,
        session: crate::services::files::media_scans::Session,
    ) -> Result<crate::services::files::media_scans::Snapshot, String> {
        let mut cancel = session.cancel.clone();
        let mut stop = self.state.stop.clone();
        let worker_cancel = cancel.clone();
        let worker_stop = stop.clone();
        let result = async {
            let mut total_files = 0usize;
            for reference in references {
                session.wait_running().await?;
                let root =
                    crate::services::files::file_access::resolve_native(&self.state, &reference)
                        .await?;
                let canonical_root = tokio::fs::canonicalize(&root)
                    .await
                    .map_err(|error| error.to_string())?;
                if !tokio::fs::symlink_metadata(&root)
                    .await
                    .map_err(|error| error.to_string())?
                    .file_type()
                    .is_dir()
                {
                    return Err("Real media scan directory required".into());
                }
                let (send, mut receive) =
                    tokio::sync::mpsc::channel::<(DataType, std::path::PathBuf)>(32);
                let directory = tokio::fs::canonicalize(&self.state.directory)
                    .await
                    .map_err(|error| error.to_string())?;
                let policy = policy.clone();
                let cancelled = worker_cancel.clone();
                let stopped = worker_stop.clone();
                let producer_root = canonical_root.clone();
                let producer = tokio::task::spawn_blocking(move || -> Result<usize, String> {
                    let mut discovered = 0;
                    plain_media::file_walk::walk_files(
                        &producer_root,
                        |path| {
                            path.to_str().is_some_and(|path| {
                                plain_media::scan::is_media_excluded_at(
                                    path,
                                    &directory,
                                    &policy.cache_directory,
                                    &policy.excluded_roots,
                                )
                            })
                        },
                        || {
                            *cancelled.borrow()
                                || cancelled.has_changed().is_err()
                                || *stopped.borrow()
                                || stopped.has_changed().is_err()
                        },
                        |path| {
                            let name = path.to_str().ok_or_else(|| {
                                anyhow::anyhow!("Invalid media file path encoding")
                            })?;
                            let kind = plain_media::scan::infer_type(name);
                            if let Some(data_type) = [
                                DataType::Audio,
                                DataType::Video,
                                DataType::Image,
                                DataType::Doc,
                            ]
                            .into_iter()
                            .find(|data_type| data_type.media_type_str() == Some(kind))
                            {
                                if data_type == DataType::Doc && std::fs::metadata(path)?.len() == 0
                                {
                                    return Ok(());
                                }
                                send.blocking_send((data_type, path.to_owned()))
                                    .map_err(|_| anyhow::anyhow!("Media scan receiver closed"))?;
                                discovered += 1;
                            }
                            Ok(())
                        },
                    )
                    .map_err(|error| error.to_string())?;
                    Ok(discovered)
                });
                while let Some((data_type, path)) = receive.recv().await {
                    session.wait_running().await?;
                    let actual = tokio::fs::canonicalize(&path)
                        .await
                        .map_err(|error| error.to_string())?;
                    if !actual.starts_with(&canonical_root) {
                        return Err("Media file escaped scan directory".into());
                    }
                    let metadata = self
                        .read_file(
                            data_type,
                            actual.to_str().ok_or("Invalid media file path encoding")?,
                        )
                        .await?;
                    if !std::path::Path::new(&metadata.metadata.path).starts_with(&canonical_root) {
                        return Err("Media file escaped scan directory".into());
                    }
                    session
                        .record_when_running(|| self.record_file_reads(&[metadata]))
                        .await?;
                }
                let total = producer.await.map_err(|error| error.to_string())??;

                total_files = total_files
                    .checked_add(total)
                    .ok_or("Media scan file count overflow")?;
            }
            session.discovered(total_files)?;
            Ok(())
        };
        let result = tokio::select! {
            biased;
            _ = crate::services::files::media_scans::wait_stop(&mut cancel) => Err("Media scan cancelled".into()),
            _ = crate::services::files::media_scans::wait_stop(&mut stop) => {
                self.state.media_scans.cancel()?;
                Err("Rust core stopped".into())
            },
            result = result => result,
        };
        session.finish(result)
    }

    #[cfg(feature = "media")]
    pub fn pause_file_scan(
        &self,
    ) -> Result<Option<crate::services::files::media_scans::Snapshot>, String> {
        self.state.native_client_id()?;
        self.state.media_scans.pause()
    }

    #[cfg(feature = "media")]
    pub fn resume_file_scan(
        &self,
    ) -> Result<Option<crate::services::files::media_scans::Snapshot>, String> {
        self.state.native_client_id()?;
        self.state.media_scans.resume()
    }

    #[cfg(feature = "media")]
    pub fn cancel_file_scan(
        &self,
    ) -> Result<Option<crate::services::files::media_scans::Snapshot>, String> {
        self.state.native_client_id()?;
        self.state.media_scans.cancel()
    }

    #[cfg(feature = "media")]
    pub fn file_scan_status(
        &self,
    ) -> Result<Option<crate::services::files::media_scans::Snapshot>, String> {
        self.state.native_client_id()?;
        self.state.media_scans.snapshot()
    }

    pub fn record_files(&self, files: &[MediaFileMetadata]) -> Result<Vec<MediaFileRow>, String> {
        self.state.native_client_id()?;
        self.state
            .db
            .record_media_files(files)
            .map_err(|error| error.to_string())
    }

    pub fn record_file_reads(
        &self,
        files: &[plain_rs::db::MediaFileMetadataRead],
    ) -> Result<Vec<MediaFileRow>, String> {
        self.state.native_client_id()?;
        self.state
            .db
            .record_media_file_reads(files)
            .map_err(|error| error.to_string())
    }

    pub fn by_path(&self, data_type: DataType, path: &str) -> Result<Option<MediaFileRow>, String> {
        self.state.native_client_id()?;
        self.state
            .db
            .media_file_by_path(data_type, path)
            .map_err(|error| error.to_string())
    }

    pub fn list(
        &self,
        data_type: DataType,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<MediaFileRow>, String> {
        self.state.native_client_id()?;
        self.state
            .db
            .media_file_list(data_type, limit, offset)
            .map_err(|error| error.to_string())
    }

    pub fn search(
        &self,
        data_type: DataType,
        query: &str,
        sort: crate::graphql::resolvers::files::types::FileSortBy,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<MediaFileRow>, String> {
        self.state.native_client_id()?;
        crate::services::files::media_catalog::rows(
            &self.state.db,
            data_type,
            query,
            sort,
            limit,
            offset,
        )
        .map_err(|error| error.to_string())
    }

    pub fn rows_facts(&self, params: serde_json::Value) -> Result<Vec<serde_json::Value>, String> {
        let request: RowsRequest =
            serde_json::from_value(params).map_err(|error| error.to_string())?;
        self.search(
            request.data_type,
            &request.query,
            request.sort_by,
            request.limit,
            request.offset,
        )?
        .iter()
        .map(crate::services::files::media_catalog_facts::row_facts)
        .collect()
    }

    pub fn bucket_item_facts(
        &self,
        params: serde_json::Value,
    ) -> Result<Vec<serde_json::Value>, String> {
        self.state.native_client_id()?;
        let request: BucketRequest =
            serde_json::from_value(params).map_err(|error| error.to_string())?;
        crate::services::files::media_catalog::bucket_item_facts(&self.state.db, request.data_type)
            .map_err(|error| error.to_string())
    }

    #[cfg(feature = "media")]
    pub fn delete_action_files(
        &self,
        params: serde_json::Value,
    ) -> Result<plain_rs::media::actions::Outcome, String> {
        use plain_rs::media::actions::{Action, Item, Outcome};
        use rusqlite::OptionalExtension;
        self.state.native_client_id()?;
        let request: DeleteActionRequest =
            serde_json::from_value(params).map_err(|error| error.to_string())?;
        if request.action != Action::Delete || !request.dest_dir.is_empty() {
            return Err("Only media delete is implemented by this provider".into());
        }
        if ![
            DataType::Audio,
            DataType::Video,
            DataType::Image,
            DataType::Doc,
        ]
        .iter()
        .any(|kind| kind.kind() == request.data_type)
            || request.ids.len() > 500
            || request.ids.iter().any(String::is_empty)
            || request
                .ids
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != request.ids.len()
        {
            return Err("Invalid media delete selection".into());
        }
        let rows = self
            .state
            .db
            .with_conn(|conn| {
                let mut statement = conn.prepare(
                    "SELECT path,is_trashed FROM media_files WHERE id=?1 AND data_type=?2",
                )?;
                request
                    .ids
                    .iter()
                    .map(|id| {
                        statement
                            .query_row(rusqlite::params![id, request.data_type], |row| {
                                Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?))
                            })
                            .optional()
                    })
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .map_err(|error| error.to_string())?;
        let mut outcome = Outcome {
            successful: Vec::new(),
            failed_ids: Vec::new(),
        };
        for (id, row) in request.ids.into_iter().zip(rows) {
            let removed = (|| -> Result<String, String> {
                self.state.native_client_id()?;
                let (path, trashed) = row.ok_or("Media file not found")?;
                if trashed != request.from_trash || !std::path::Path::new(&path).is_absolute() {
                    return Err("Media delete path or trash state mismatch".into());
                }
                self.state.media_scans.record_completion(|| {
                    let metadata =
                        std::fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
                    if !metadata.file_type().is_file() {
                        return Err("Regular media file required".into());
                    }
                    std::fs::remove_file(&path).map_err(|error| error.to_string())?;
                    let kind = [
                        DataType::Audio,
                        DataType::Video,
                        DataType::Image,
                        DataType::Doc,
                    ]
                    .into_iter()
                    .find(|kind| kind.kind() == request.data_type)
                    .ok_or("Invalid media delete type")?;
                    let receipt = Item {
                        id: id.clone(),
                        path: path.clone(),
                        destination_path: String::new(),
                    };
                    let cleanup = |db: &plain_rs::db::Db| {
                        plain_rs::media::actions::cleanup(db, kind, Action::Delete, &[receipt])
                    };
                    let result = if kind == DataType::Image {
                        self.state.image_index.update_cache(cleanup)
                    } else {
                        cleanup(&self.state.db)
                    };
                    result.map(|_| ()).map_err(|error| {
                        format!("Media file removed but catalog cleanup failed: {error}")
                    })
                })?;
                Ok(path)
            })();
            match removed {
                Ok(path) => outcome.successful.push(Item {
                    id,
                    path,
                    destination_path: String::new(),
                }),
                Err(error) => {
                    log::warn!("Media delete failed for {id}: {error}");
                    outcome.failed_ids.push(id);
                }
            }
        }
        if !outcome.successful.is_empty() {
            let _ = self.state.events.send(plain_rs::events::WsEvent::broadcast(
                "CONTENT_CHANGED",
                "{}".into(),
            ));
        }
        Ok(outcome)
    }

    pub fn ids_facts(&self, params: serde_json::Value) -> Result<Vec<String>, String> {
        self.state.native_client_id()?;
        let request: IdsRequest =
            serde_json::from_value(params).map_err(|error| error.to_string())?;
        crate::services::files::media_catalog::ids(
            &self.state.db,
            request.data_type,
            &request.query,
            request.from_trash,
        )
        .map_err(|error| error.to_string())
    }

    pub fn count_facts(&self, params: serde_json::Value) -> Result<i64, String> {
        let request: CountRequest =
            serde_json::from_value(params).map_err(|error| error.to_string())?;
        self.count_matching(request.data_type, &request.query)
    }

    pub fn count_matching(&self, data_type: DataType, query: &str) -> Result<i64, String> {
        self.state.native_client_id()?;
        crate::services::files::media_catalog::count(&self.state.db, data_type, query)
            .map_err(|error| error.to_string())
    }

    pub fn count(&self, data_type: DataType) -> Result<i64, String> {
        self.state.native_client_id()?;
        self.state
            .db
            .media_file_count(data_type)
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/native/media_catalog.rs"]
mod tests;
