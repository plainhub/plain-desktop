#[derive(Clone, Debug)]
pub struct MediaFileMetadata {
    pub data_type: crate::enums::DataType,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub bucket_id: String,
    pub size_bytes: i64,
    pub duration_ms: i64,
    pub album_file_id: String,
    pub width: i32,
    pub height: i32,
    pub rotation: i32,
    pub taken_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MediaFileMetadataAvailability {
    pub tags: bool,
    pub duration: bool,
    pub dimensions: bool,
    pub rotation: bool,
    pub taken_at: bool,
    pub album_file_id: bool,
}

impl MediaFileMetadataAvailability {
    pub const ALL: Self = Self {
        tags: true,
        duration: true,
        dimensions: true,
        rotation: true,
        taken_at: true,
        album_file_id: true,
    };
}

#[derive(Clone, Debug)]
pub struct MediaFileMetadataRead {
    pub metadata: MediaFileMetadata,
    pub available: MediaFileMetadataAvailability,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct MediaFileRow {
    pub id: String,
    pub data_type: i32,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub bucket_id: String,
    pub size_bytes: i64,
    pub duration_ms: i64,
    pub album_file_id: String,
    pub width: i32,
    pub height: i32,
    pub rotation: i32,
    pub taken_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub is_favorite: bool,
    pub is_trashed: bool,
    pub original_path: Option<String>,
}
