//! GraphQL output types of the media surface — the plain-app / plain-nas
//! contract dialect (UPPER_SNAKE enums, `Long` / `Instant` scalars).
//! The shared plain-rs API merges these with `MediaQueryRoot` /
//! `MediaMutationRoot` for desktop and NAS.

use async_graphql::{Enum, ID, InputObject, SimpleObject};

/// The Long scalar type represents a signed 64-bit numeric non-fractional
/// value. Serialized as a JSON number — GraphQL `Int` is only 32-bit, so
/// byte counts and millisecond durations that can exceed 2^31 use this.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Long(pub i64);

#[async_graphql::Scalar]
impl async_graphql::ScalarType for Long {
    fn parse(value: async_graphql::Value) -> async_graphql::InputValueResult<Self> {
        if let async_graphql::Value::Number(n) = &value
            && let Some(i) = n.as_i64()
        {
            return Ok(Long(i));
        }
        Err(async_graphql::InputValueError::expected_type(value))
    }

    fn to_value(&self) -> async_graphql::Value {
        async_graphql::Value::from(self.0)
    }
}

/// ISO-8601 / RFC 3339 UTC timestamp string, e.g. 2026-09-20T12:34:56.789Z
/// (plain-app `Instant` scalar).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Instant(pub chrono::DateTime<chrono::Utc>);

#[async_graphql::Scalar]
impl async_graphql::ScalarType for Instant {
    fn parse(value: async_graphql::Value) -> async_graphql::InputValueResult<Self> {
        if let async_graphql::Value::String(s) = &value
            && let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s)
        {
            return Ok(Instant(dt.with_timezone(&chrono::Utc)));
        }
        Err(async_graphql::InputValueError::expected_type(value))
    }

    fn to_value(&self) -> async_graphql::Value {
        async_graphql::Value::String(self.0.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
    }
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
pub enum TrashSortBy {
    DATE_ASC,
    DATE_DESC,
    SIZE_ASC,
    SIZE_DESC,
    NAME_ASC,
    NAME_DESC,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct TrashItem {
    pub id: ID,
    pub r#type: TrashItemType,
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
    pub trashed_path: String, // physical path under .nas-trash
}

/// Kind of a trashed entry (stored kind string `"file"` | `"dir"`).
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types, clippy::upper_case_acronyms)] // SDL contract names
pub enum TrashItemType {
    FILE,
    DIR,
}

impl TrashItemType {
    pub fn from_kind(kind: &str) -> Option<Self> {
        match kind {
            "file" => Some(Self::FILE),
            "dir" => Some(Self::DIR),
            _ => None,
        }
    }
}

use crate::enums::DataType;

/// Media-library item kinds accepted by media item actions and buckets
/// (plain-app `MediaDataType`).
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types)] // plain-app SDL contract names
pub enum MediaDataType {
    AUDIO,
    VIDEO,
    IMAGE,
    DOC,
}

impl MediaDataType {
    pub fn data_type(self) -> DataType {
        match self {
            MediaDataType::DOC => DataType::Doc,
            MediaDataType::AUDIO => DataType::Audio,
            MediaDataType::VIDEO => DataType::Video,
            MediaDataType::IMAGE => DataType::Image,
        }
    }
}

#[derive(SimpleObject, Clone, Debug)]
pub struct Tag {
    pub id: ID,
    pub name: String,
    /// Numeric tag kind — the ordinal of the tag's `DataType`
    /// (DEFAULT=0, AUDIO=1, VIDEO=2, IMAGE=3, …). The NAS only has data
    /// for the media kinds; the Int is frozen phone contract (API_SPEC
    /// §9), the mapping lives in docs/api/tags.md.
    pub r#type: i32,
    pub count: i32,
}

#[derive(SimpleObject, InputObject, Clone, Debug)]
pub struct TagRelationStub {
    pub key: String,
    pub title: String,
    pub size: Long,
}

/// One (tag, item-key) relation (plain-app contract). `key` is the media id
/// / entity key the tag is attached to.
#[derive(SimpleObject, Clone, Debug)]
pub struct TagRelation {
    #[graphql(name = "tagId")]
    pub tag_id: ID,
    pub key: String,
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

/// Audio (media index).
#[derive(SimpleObject, Clone, Debug)]
#[graphql(complex)]
pub struct Audio {
    pub id: ID,
    pub title: String,
    pub artist: String,
    pub path: String,
    #[graphql(name = "durationMs")]
    pub duration_ms: Long,
    pub size: Long,
    pub bucket_id: ID,
    pub album_file_id: String,
    pub created_at: Instant,
    pub updated_at: Instant,
    /// plain-app serves the per-track favorite flag; the NAS has no
    /// per-media favorites (folders only), so this is always false.
    pub is_favorite: bool,
}

/// Media-index scan lifecycle, shared by `scanProgress.state` and the
/// `media:scan:progress` WS payload (same values on both wires).
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types, clippy::upper_case_acronyms)] // SDL contract names
pub enum ScanState {
    IDLE,
    RUNNING,
    PAUSED,
    STOPPED,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct ScanProgress {
    pub indexed: i32,
    pub pending: i32,
    pub total: i32,
    pub state: ScanState,
}

/// AI image-search capability status. No CLIP model runtime ships here,
/// so this always reports `UNAVAILABLE`; the field exists so clients
/// probing the phone-style AI search surface get a well-formed answer
/// instead of a GraphQL error.
#[derive(SimpleObject, Clone, Debug)]
pub struct ImageSearchStatus {
    pub status: ImageSearchStatusType,
    pub download_progress: i32,
    pub error_message: String,
    pub model_size: Long,
    pub model_dir: String,
    pub is_indexing: bool,
    pub total_images: i32,
    pub indexed_images: i32,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum ImageSearchStatusType {
    Unavailable,
    Downloading,
    Loading,
    Ready,
    Error,
}

#[derive(SimpleObject, Clone, Debug)]
#[graphql(complex)]
pub struct Image {
    pub id: ID,
    pub title: String,
    pub path: String,
    pub size: Long,
    #[graphql(name = "bucketId")]
    pub bucket_id: ID,
    /// Capture time; no EXIF extraction yet, so it mirrors the file mtime.
    #[graphql(name = "takenAt")]
    pub taken_at: Option<Instant>,
    #[graphql(name = "createdAt")]
    pub created_at: Instant,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
    /// No per-media favorites on NAS — always false (plain-app parity).
    #[graphql(name = "isFavorite")]
    pub is_favorite: bool,
}

#[derive(SimpleObject, Clone, Debug)]
#[graphql(complex)]
pub struct Video {
    pub id: ID,
    pub title: String,
    pub path: String,
    #[graphql(name = "durationMs")]
    pub duration_ms: Long,
    pub size: Long,
    #[graphql(name = "bucketId")]
    pub bucket_id: ID,
    /// Capture time; container metadata extraction lands later, so it
    /// mirrors the file mtime for now.
    #[graphql(name = "takenAt")]
    pub taken_at: Option<Instant>,
    #[graphql(name = "createdAt")]
    pub created_at: Instant,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
    /// No per-media favorites on NAS — always false (plain-app parity).
    #[graphql(name = "isFavorite")]
    pub is_favorite: bool,
}

#[derive(SimpleObject, Clone, Debug)]
#[graphql(complex)]
pub struct Doc {
    pub id: ID,
    pub title: String,
    pub path: String,
    /// Lowercased filename extension (plain-app `getFilenameExtension`).
    pub extension: String,
    pub size: Long,
    #[graphql(name = "bucketId")]
    pub bucket_id: ID,
    #[graphql(name = "createdAt")]
    pub created_at: Instant,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
}

/// One extension group of the docs sidebar (plain-app `DocExtGroup`).
#[derive(SimpleObject, Clone, Debug)]
pub struct DocExtGroup {
    pub ext: String,
    pub count: i32,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct MediaBucket {
    pub id: ID,
    pub name: String,
    #[graphql(name = "itemCount")]
    pub item_count: i32,
    #[graphql(name = "topItemPaths")]
    pub top_item_paths: Vec<String>,
}

/// Bulk-operation outcome (plain-app contract): how many entities the
/// mutation actually affected.
#[derive(SimpleObject, Clone, Debug)]
pub struct ActionResult {
    #[graphql(name = "affectedCount")]
    pub affected_count: i32,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types, clippy::upper_case_acronyms)] // SDL contract names
pub enum PathKind {
    FILE,
    DIR,
}
