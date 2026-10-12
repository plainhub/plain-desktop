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

#[derive(Clone)]
pub struct NativeMediaCatalog {
    state: ServerState,
}

impl NativeMediaCatalog {
    pub(crate) fn new(state: ServerState) -> Self {
        Self { state }
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
                let metadata = tokio::select! {
                    biased;
                    _ = crate::services::files::media_scans::wait_stop(&mut cancel) => return Err("Media scan cancelled".into()),
                    _ = crate::services::files::media_scans::wait_stop(&mut stop) => {
                        self.state.media_scans.cancel()?;
                        return Err("Rust core stopped".into());
                    },
                    metadata = self.read_file(data_type, &reference) => metadata?,
                };
                session.record(|| self.record_file_reads(&[metadata]))?;
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
        self.state.native_client_id()?;
        let policy = self.state.media_scans.policy()?;
        let session = self.state.media_scans.begin_discovery()?;
        let mut cancel = session.cancel.clone();
        let mut stop = self.state.stop.clone();
        let worker_cancel = cancel.clone();
        let worker_stop = stop.clone();
        let result = async {
            let root =
                crate::services::files::file_access::resolve_native(&self.state, reference).await?;
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
                        let name = path
                            .to_str()
                            .ok_or_else(|| anyhow::anyhow!("Invalid media file path encoding"))?;
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
                            if data_type == DataType::Doc && std::fs::metadata(path)?.len() == 0 {
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
                session.record(|| self.record_file_reads(&[metadata]))?;
            }
            let total = producer.await.map_err(|error| error.to_string())??;
            session.discovered(total)?;
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
