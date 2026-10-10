use crate::assembly::enums::DriveType;
use crate::graphql::resolvers::audio::desktop_types::Audio;
use crate::graphql::resolvers::images::types::Image;
use crate::graphql::resolvers::videos::types::Video;
use crate::graphql::types::Instant;
use crate::graphql::types::Long;
use async_graphql::Enum;
use async_graphql::ID;
use async_graphql::InputObject;
use async_graphql::SimpleObject;
use async_graphql::Union;

/// Chunked-upload merge job state (plain-app `MergeTaskStatus`).
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[graphql(rename_items = "SCREAMING_SNAKE_CASE")]
pub enum MergeTaskStatus {
    None,
    Started,
    Merging,
    Done,
    Failed,
}

/// Chunked-upload merge job (plain-app `MergeTask`): `value` is the merged
/// result's base name once `DONE`; `merged_size` its size in bytes.
#[derive(SimpleObject, Clone, Debug)]
pub struct MergeTask {
    pub status: MergeTaskStatus,
    pub value: Option<String>,
    pub merged_size: Option<Long>,
    pub error: Option<String>,
}

/// All fields from both `homeStatsGQL` and the full `mountsGQL` query.
#[derive(SimpleObject)]
pub struct Mount {
    pub id: ID,
    pub name: String,
    pub path: String,
    pub mount_point: String,
    pub fs_type: String,
    pub total_bytes: Long,
    pub used_bytes: Long,
    pub free_bytes: Long,
    pub remote: bool,
    pub alias: String,
    pub drive_type: DriveType,
    pub disk_id: String,
}

/// EXIF / video GPS coordinate pair. Mirrors plain-app `Location`.
#[derive(SimpleObject, Clone)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(SimpleObject)]
pub struct ImageFileInfo {
    pub width: i32,
    pub height: i32,
    pub location: Option<Location>,
}

#[derive(SimpleObject)]
pub struct VideoFileInfo {
    pub width: i32,
    pub height: i32,
    /// Milliseconds. `0` in local mode — the desktop local server has no
    /// `MediaMetadataRetriever` equivalent. Main-window traffic still goes
    /// through the device server and returns real durations.
    pub duration_ms: Long,
    pub location: Option<Location>,
}

#[derive(SimpleObject)]
pub struct AudioFileInfo {
    /// Milliseconds. `0` in local mode (see `VideoFileInfo::duration_ms`).
    pub duration_ms: Long,
    pub location: Option<Location>,
}

/// Polymorphic media-metadata payload — clients select via `... on XFileInfo`.
#[derive(Union)]
pub enum MediaFileInfo {
    Image(ImageFileInfo),
    Video(VideoFileInfo),
    Audio(AudioFileInfo),
}

/// `fileInfo` query result. `data` is `None` for non-media files.
#[derive(SimpleObject)]
#[graphql(complex)]
pub struct FileInfo {
    pub path: String,
    #[graphql(skip)]
    pub updated_at: String,
    pub size: Long,
    pub data: Option<MediaFileInfo>,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct File {
    pub name: String,
    pub path: String,
    pub created_at: Option<Instant>,
    pub updated_at: Instant,
    pub size: Long,
    pub is_dir: bool,
    /// Direct-children count for dirs, 0 for files (plain-app contract name).
    pub child_count: i32,
    /// Android MediaStore id elsewhere; a NAS has no media store so null.
    pub media_id: Option<ID>,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types)]
pub enum FileSortBy {
    DATE_ASC,
    DATE_DESC,
    SIZE_ASC,
    SIZE_DESC,
    NAME_ASC,
    NAME_DESC,
    TAKEN_AT_DESC,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types)]
pub enum TrashedFileSortBy {
    DATE_ASC,
    DATE_DESC,
    SIZE_ASC,
    SIZE_DESC,
    NAME_ASC,
    NAME_DESC,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct TrashedFile {
    pub id: ID,
    pub r#type: TrashedFileType,
    pub original_path: String,
    pub disk: String,
    pub trash_rel_path: String,
    pub deleted_at: Instant,
    pub uid: i32,
    pub gid: i32,
    pub mode: i32,
    /// Byte size of the trashed entry (files only; null for dirs).
    #[graphql(name = "sizeBytes")]
    pub size_bytes: Option<Long>,
    pub entry_count: Option<i32>,
    pub display_name: String, // base name of original_path
    pub trashed_path: String, // physical path under .plain-trash
}

/// Kind of a trashed entry (stored kind string `"file"` | `"dir"`).
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types, clippy::upper_case_acronyms)] // SDL contract names
pub enum TrashedFileType {
    FILE,
    DIR,
}

impl TrashedFileType {
    pub fn from_kind(kind: &str) -> Option<Self> {
        match kind {
            "file" => Some(Self::FILE),
            "dir" => Some(Self::DIR),
            _ => None,
        }
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types)]
pub enum FileTaskType {
    COPY,
    MOVE,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types)]
pub enum FileTaskStatus {
    QUEUED,
    RUNNING,
    DONE,
    ERROR,
}

#[derive(InputObject, Clone, Debug)]
pub struct FileTaskOpInput {
    pub src: String,
    pub dst: String,
    pub overwrite: bool,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct FileTask {
    pub id: ID,
    pub r#type: FileTaskType,
    pub title: String,
    pub status: FileTaskStatus,
    pub error: String,
    pub total_bytes: Long,
    pub done_bytes: Long,
    pub total_items: i32,
    pub done_items: i32,
    pub created_at: Instant,
    pub updated_at: Instant,
}

/// Bulk-operation outcome (plain-app contract): how many entities the
/// mutation actually affected.

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types, clippy::upper_case_acronyms)] // SDL contract names
pub enum PathKind {
    FILE,
    DIR,
}

#[async_graphql::ComplexObject]
impl FileInfo {
    async fn updated_at(&self) -> async_graphql::Result<crate::graphql::types::Instant> {
        crate::graphql::types::parse_instant(&self.updated_at)
    }
}
