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
        self.record_file_reads(&[metadata])?
            .into_iter()
            .next()
            .ok_or_else(|| "Missing recorded media file".into())
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
