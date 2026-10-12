use crate::assembly::server::ServerState;
use plain_rs::db::{MediaFileMetadata, MediaFileRow};
use plain_rs::enums::DataType;

#[derive(Clone)]
pub struct NativeMediaCatalog {
    state: ServerState,
}

impl NativeMediaCatalog {
    pub(crate) fn new(state: ServerState) -> Self {
        Self { state }
    }

    pub fn record_files(&self, files: &[MediaFileMetadata]) -> Result<Vec<MediaFileRow>, String> {
        self.state.native_client_id()?;
        self.state
            .db
            .record_media_files(files)
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
