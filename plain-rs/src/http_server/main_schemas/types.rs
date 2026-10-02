pub use crate::content_types::{ActionResult, Instant, Long, Tag, TagRelation, TagRelationStub};
use async_graphql::{ComplexObject, Enum, ID, InputObject, SimpleObject, Union};

use crate::api::db::{DChannel, DChat, DPeer};
use crate::api::enums::{
    AppChannelType, ChannelStatus, ChatStatus, DeviceType, DriveType, MemberStatus, PeerStatus,
};
// ── Output types ──────────────────────────────────────────────────────────────

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
#[graphql(name = "DevicePlatform")]
pub enum DevicePlatform {
    Android,
    Ios,
    Macos,
    Windows,
    Linux,
}

#[derive(SimpleObject)]
#[graphql(name = "AndroidExtras")]
pub struct AndroidExtras {
    pub sdk_version: i32,
    pub version_code_name: String,
    pub security_patch: String,
    pub bootloader: String,
    pub fingerprint: String,
    pub hardware: String,
    pub radio_version: String,
    pub board: String,
    pub build_brand: String,
    pub build_number: String,
    pub device: String,
    pub java_vm_version: String,
    pub gl_es_version: String,
    pub build_time: Instant,
}

#[derive(SimpleObject)]
#[graphql(name = "DisplayInfo")]
pub struct DisplayInfo {
    pub width: i32,
    pub height: i32,
    pub density: f64,
}

#[derive(SimpleObject)]
#[graphql(name = "DeviceInfo")]
pub struct DeviceInfo {
    pub name: String,
    pub platform: DevicePlatform,
    pub manufacturer: String,
    pub model: String,
    pub os_name: String,
    pub os_version: String,
    pub kernel_version: String,
    pub app_version: String,
    pub app_build_number: String,
    pub language: String,
    pub cpu_arch: String,
    pub cpu_model: Option<String>,
    pub total_memory: Long,
    pub total_storage: Long,
    pub display: Option<DisplayInfo>,
    pub android: Option<AndroidExtras>,
}

#[derive(SimpleObject)]
#[graphql(name = "Temperature")]
pub struct Temperature {
    pub label: String,
    pub celsius: f64,
}

#[derive(SimpleObject, Default)]
#[graphql(name = "DeviceStatus")]
pub struct DeviceStatus {
    pub uptime_sec: Long,
    /// 0-100; None when the device has no battery or the level is unknown.
    pub battery_level: Option<i32>,
    /// True only while actively charging; full-while-plugged is false.
    pub charging: bool,
    /// Empty when the platform exposes no temperature sources.
    pub temperatures: Vec<Temperature>,
    /// 0-100 percent, diffed from two CPU counter samples.
    pub cpu_usage: f64,
    /// OS-level available memory; None when the platform does not expose it.
    pub memory_available: Option<Long>,
    /// Available bytes on the primary data volume.
    pub storage_available: Long,
}

#[derive(SimpleObject)]
#[graphql(name = "Sim")]
pub struct Sim {
    pub id: String,
    pub label: String,
    pub number: String,
    pub subscription_id: i32,
}

// BatteryHealth / BatteryStatus / BatteryPlugged / Battery were removed:
// dynamic battery state now lives in DeviceStatus (batteryLevel + charging).

#[derive(SimpleObject)]
pub struct FavoriteFolder {
    pub root_path: String,
    pub full_path: String,
    pub alias: Option<String>,
}

#[derive(SimpleObject)]
pub struct KeyValuePair {
    pub key: String,
    pub value: String,
}

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

/// One track of the playback queue / a playlist (plain-app `AudioItem`).
/// Stored durations are seconds; the wire field is milliseconds.
#[derive(SimpleObject, Clone, Debug)]
pub struct AudioItem {
    pub title: String,
    pub artist: String,
    pub path: String,
    #[graphql(name = "durationMs")]
    pub duration_ms: Long,
}

/// A user playlist (plain-app `AudioPlaylist`); `itemCount` is live.
#[derive(SimpleObject, Clone, Debug)]
#[graphql(complex)]
pub struct AudioPlaylist {
    pub id: ID,
    pub name: String,
    pub item_count: i32,
    #[graphql(skip)]
    pub created_at: String,
    #[graphql(skip)]
    pub updated_at: String,
}

/// Recently played track (plain-app `AudioPlayHistory`).
#[derive(SimpleObject, Clone, Debug)]
#[graphql(complex)]
pub struct AudioPlayHistory {
    pub path: String,
    pub title: String,
    pub artist: String,
    #[graphql(name = "durationMs")]
    pub duration_ms: Long,
    pub play_count: i64,
    #[graphql(skip)]
    pub played_at: String,
}

/// Audio player state (plain-app `AudioPlayback`). The desktop backend has
/// no playback engine — the queue is the shared state machine, the client
/// renders audio.
#[derive(SimpleObject)]
#[graphql(name = "AudioPlayback")]
pub struct AudioPlayback {
    pub mode: crate::api::enums::MediaPlayMode,
    pub current_path: Option<String>,
    pub is_playing: bool,
    pub position_ms: Long,
}

/// Optional capabilities the server declares about itself; the web client
/// gates UI on these instead of sniffing OS versions. Mirrors plain-app
/// `Capability`.
#[derive(Enum, Copy, Clone, Eq, PartialEq)]
#[graphql(rename_items = "SCREAMING_SNAKE_CASE")]
pub enum Capability {
    MediaTrash,
    MirrorAudio,
    DocPreview,
    ImageSearch,
    MediaScan,
    Sms,
    Calls,
    CallPhone,
    Contacts,
    Packages,
    Notes,
    Feeds,
    ScreenMirror,
    ImageEditor,
    Notifications,
    Clipboard,
    Pomodoro,
    LanShare,
    DiskManager,
}

#[derive(SimpleObject)]
pub struct App {
    pub client_id: String,
    pub url_token: String,
    pub http_port: i32,
    pub https_port: i32,
    pub app_dir: String,
    pub device_name: String,
    pub device_type: DeviceType,
    pub capabilities: Vec<Capability>,
    pub build_channel: AppChannelType,
    pub permissions: Vec<crate::api::enums::Permission>,
    pub downloads_dir: String,
    pub developer_mode: bool,
    pub debug: bool,
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

// ── fileInfo query (mirrors plain-app web/models/FileInfo.kt) ────────────────
//
// Schema shape is what the web lightbox's `fileInfoGQL` query expects. The
// `data` field is a polymorphic union over Image/Video/AudioFileInfo so the
// client's `... on ImageFileInfo { width height location { ... } }` fragment
// stays valid. Local-mode `tags` and `video/audio` metadata are best-effort
// (zeros / empty); the popup window's right-side info panel is collapsed by
// default and main-window traffic still goes through the device server.

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

#[derive(SimpleObject)]
#[graphql(complex)]
pub struct ChatItem {
    pub id: ID,
    pub from_id: ID,
    pub to_id: ID,
    pub channel_id: Option<ID>,
    #[graphql(skip)]
    pub created_at: String,
    #[graphql(skip)]
    pub updated_at: String,
    pub content: String,
    pub status: ChatStatus,
    pub status_data: String,
}

impl From<DChat> for ChatItem {
    fn from(c: DChat) -> Self {
        Self {
            id: c.id.into(),
            from_id: c.from_id.into(),
            to_id: c.to_id.into(),
            channel_id: if c.channel_id.is_empty() {
                None
            } else {
                Some(c.channel_id.into())
            },
            created_at: c.created_at,
            updated_at: c.updated_at,
            content: c.content,
            status: c.status.into(),
            status_data: c.status_data,
        }
    }
}

#[derive(SimpleObject, Clone)]
pub struct ChatChannelMember {
    pub peer_id: ID,
    pub status: MemberStatus,
}

#[derive(SimpleObject)]
#[graphql(complex)]
pub struct ChatChannel {
    pub id: ID,
    pub name: String,
    pub owner_id: ID,
    pub members: Vec<ChatChannelMember>,
    pub version: Long,
    pub status: ChannelStatus,
    #[graphql(skip)]
    pub created_at: String,
    #[graphql(skip)]
    pub updated_at: String,
}

impl From<DChannel> for ChatChannel {
    fn from(ch: DChannel) -> Self {
        let members = crate::chat::channel::messages::decode_members(&ch.members)
            .into_iter()
            .map(|m| ChatChannelMember {
                peer_id: m.peer_id.into(),
                status: m.status.into(),
            })
            .collect();
        Self {
            id: ch.id.into(),
            name: ch.name,
            owner_id: ch.owner_id.into(),
            members,
            version: Long(ch.version),
            status: ch.status.into(),
            created_at: ch.created_at,
            updated_at: ch.updated_at,
        }
    }
}

#[derive(SimpleObject, serde::Serialize)]
#[graphql(complex)]
#[serde(rename_all = "camelCase")]
pub struct Peer {
    pub id: ID,
    pub name: String,
    pub ip: String,
    pub status: PeerStatus,
    pub online: bool,
    pub port: i32,
    pub device_type: DeviceType,
    #[graphql(skip)]
    pub token: String,
    #[graphql(skip)]
    pub public_key: String,
    #[graphql(skip)]
    pub created_at: String,
    #[graphql(skip)]
    pub updated_at: String,
}

impl Peer {
    /// Build the GraphQL `Peer` from a `DPeer` row plus the live online flag
    /// from `PeerStatusManager`. Mirrors plain-app's
    /// `DPeer.toModel()` which calls `PeerStatusManager.isOnline(id)`.
    pub fn from_dpeer(p: DPeer, online: bool) -> Self {
        Self {
            id: p.id.into(),
            name: p.name,
            ip: p.ip,
            status: p.status.into(),
            online,
            port: p.port as i32,
            device_type: p.device_type.into(),
            token: p.token,
            public_key: p.public_key,
            created_at: p.created_at,
            updated_at: p.updated_at,
        }
    }
}

pub use super::bookmark_types::{Bookmark, BookmarkGroup, BookmarkInput};

pub(crate) fn parse_instant(value: &str) -> async_graphql::Result<Instant> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|dt| Instant(dt.with_timezone(&chrono::Utc)))
        .map_err(|e| async_graphql::Error::new(format!("invalid stored timestamp: {e}")))
}

macro_rules! instant_fields {
    ($name:ident, $($field:ident),+ $(,)?) => {
        #[ComplexObject]
        impl $name {
            $(async fn $field(&self) -> async_graphql::Result<Instant> {
                parse_instant(&self.$field)
            })+
        }
    };
}

instant_fields!(AudioPlaylist, created_at, updated_at);
instant_fields!(AudioPlayHistory, played_at);
instant_fields!(FileInfo, updated_at);
instant_fields!(ChatItem, created_at, updated_at);
instant_fields!(ChatChannel, created_at, updated_at);
instant_fields!(Peer, created_at, updated_at);

// Media GraphQL scalar and output types.

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
    pub trashed_path: String, // physical path under .nas-trash
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

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types, clippy::upper_case_acronyms)] // SDL contract names
pub enum PathKind {
    FILE,
    DIR,
}
