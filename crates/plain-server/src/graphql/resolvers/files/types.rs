use crate::graphql::types::Instant;
use crate::graphql::types::Long;
use async_graphql::Enum;
use async_graphql::ID;
use async_graphql::SimpleObject;

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum DriveType {
    #[default]
    InternalStorage,
    Sdcard,
    UsbStorage,
    App,
}

impl DriveType {
    pub(crate) fn parse(value: &str) -> Self {
        match value {
            "SDCARD" => Self::Sdcard,
            "USB_STORAGE" => Self::UsbStorage,
            "APP" => Self::App,
            _ => Self::InternalStorage,
        }
    }
}

/// A mounted volume. The phone reports one row per storage the platform
/// knows about (internal, SD card, each USB disk, the app-private dir);
/// the fields are all filled by the host, never derived here.
#[derive(SimpleObject, Clone, Debug)]
pub struct Mount {
    pub id: ID,
    pub name: String,
    pub path: String,
    #[graphql(name = "mountPoint")]
    pub mount_point: String,
    #[graphql(name = "fsType")]
    pub fs_type: String,
    #[graphql(name = "totalBytes")]
    pub total_bytes: Long,
    #[graphql(name = "usedBytes")]
    pub used_bytes: Long,
    #[graphql(name = "freeBytes")]
    pub free_bytes: Long,
    pub remote: bool,
    pub alias: String,
    #[graphql(name = "driveType")]
    pub drive_type: DriveType,
    #[graphql(name = "diskId")]
    pub disk_id: String,
}

/// One entry of a directory listing or a search page.
#[derive(SimpleObject, Clone, Debug)]
pub struct File {
    /// Empty for non-media files — exposed as `null` rather than an empty id.
    #[graphql(name = "mediaId")]
    pub media_id: Option<ID>,
    pub name: String,
    pub path: String,
    #[graphql(name = "createdAt")]
    pub created_at: Option<Instant>,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
    pub size: Long,
    #[graphql(name = "isDir")]
    pub is_dir: bool,
    #[graphql(name = "childCount")]
    pub child_count: i32,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct ImageFileInfo {
    pub width: i32,
    pub height: i32,
    pub location: Option<Location>,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct AudioFileInfo {
    #[graphql(name = "durationMs")]
    pub duration_ms: Long,
    pub location: Option<Location>,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct VideoFileInfo {
    pub width: i32,
    pub height: i32,
    #[graphql(name = "durationMs")]
    pub duration_ms: Long,
    pub location: Option<Location>,
}

#[derive(async_graphql::Union, Clone, Debug)]
pub enum MediaFileInfo {
    Image(ImageFileInfo),
    Audio(AudioFileInfo),
    Video(VideoFileInfo),
}

/// Stat plus the media probe for one path. `data` is `null` when the name
/// is not a media extension, which is why it is a union and not a
/// single struct.
#[derive(SimpleObject, Clone, Debug)]
pub struct FileInfo {
    pub path: String,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
    pub size: Long,
    pub data: Option<MediaFileInfo>,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FileSortBy {
    DateAsc,
    DateDesc,
    SizeAsc,
    SizeDesc,
    NameAsc,
    NameDesc,
    TakenAtDesc,
}

impl FileSortBy {
    pub(crate) fn from_ordinal(value: i32) -> async_graphql::Result<Self> {
        Ok(match value {
            0 => Self::DateAsc,
            1 => Self::DateDesc,
            2 => Self::SizeAsc,
            3 => Self::SizeDesc,
            4 => Self::NameAsc,
            5 => Self::NameDesc,
            6 => Self::TakenAtDesc,
            _ => return Err("invalid file sort order".into()),
        })
    }

    /// The wire name the search DSL and the platform layer sort by. It
    /// doubles as the `sortBy` value the host provider route accepts.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            FileSortBy::DateAsc => "DATE_ASC",
            FileSortBy::DateDesc => "DATE_DESC",
            FileSortBy::SizeAsc => "SIZE_ASC",
            FileSortBy::SizeDesc => "SIZE_DESC",
            FileSortBy::NameAsc => "NAME_ASC",
            FileSortBy::NameDesc => "NAME_DESC",
            FileSortBy::TakenAtDesc => "TAKEN_AT_DESC",
        }
    }
}
