use async_graphql::{Enum, InputObject, SimpleObject, Union};

use crate::api::db::{DBookmark, DBookmarkGroup, DChannel, DChat, DPeer};
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
    pub build_time: String,
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
    pub total_memory: i64,
    pub total_storage: i64,
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
    pub uptime_sec: i64,
    /// 0-100; None when the device has no battery or the level is unknown.
    pub battery_level: Option<i32>,
    /// True only while actively charging; full-while-plugged is false.
    pub charging: bool,
    /// Empty when the platform exposes no temperature sources.
    pub temperatures: Vec<Temperature>,
    /// 0-100 percent, diffed from two CPU counter samples.
    pub cpu_usage: f64,
    /// OS-level available memory; None when the platform does not expose it.
    pub memory_available: Option<i64>,
    /// Available bytes on the primary data volume.
    pub storage_available: i64,
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

pub use super::media::types::ActionResult;

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
    pub merged_size: Option<i64>,
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
    pub duration_ms: i64,
}

/// A user playlist (plain-app `AudioPlaylist`); `itemCount` is live.
#[derive(SimpleObject, Clone, Debug)]
pub struct AudioPlaylist {
    pub id: String,
    pub name: String,
    pub item_count: i32,
    pub created_at: String,
    pub updated_at: String,
}

/// Recently played track (plain-app `AudioPlayHistory`).
#[derive(SimpleObject, Clone, Debug)]
pub struct AudioPlayHistory {
    pub path: String,
    pub title: String,
    pub artist: String,
    #[graphql(name = "durationMs")]
    pub duration_ms: i64,
    pub play_count: i64,
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
    pub position_ms: i64,
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
    pub id: String,
    pub name: String,
    pub path: String,
    pub mount_point: String,
    pub fs_type: String,
    pub total_bytes: i64,
    pub used_bytes: i64,
    pub free_bytes: i64,
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
    pub duration_ms: i64,
    pub location: Option<Location>,
}

#[derive(SimpleObject)]
pub struct AudioFileInfo {
    /// Milliseconds. `0` in local mode (see `VideoFileInfo::duration_ms`).
    pub duration_ms: i64,
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
pub struct FileInfo {
    pub path: String,
    pub updated_at: String,
    pub size: i64,
    pub data: Option<MediaFileInfo>,
}

#[derive(SimpleObject)]
pub struct ChatItem {
    pub id: String,
    pub from_id: String,
    pub to_id: String,
    pub channel_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub content: String,
    pub status: ChatStatus,
    pub status_data: String,
}

impl From<DChat> for ChatItem {
    fn from(c: DChat) -> Self {
        Self {
            id: c.id,
            from_id: c.from_id,
            to_id: c.to_id,
            channel_id: if c.channel_id.is_empty() {
                None
            } else {
                Some(c.channel_id)
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
    pub peer_id: String,
    pub status: MemberStatus,
}

#[derive(SimpleObject)]
pub struct ChatChannel {
    pub id: String,
    pub name: String,
    pub owner_id: String,
    pub members: Vec<ChatChannelMember>,
    pub version: i64,
    pub status: ChannelStatus,
    pub created_at: String,
    pub updated_at: String,
}

impl From<DChannel> for ChatChannel {
    fn from(ch: DChannel) -> Self {
        let members = crate::chat::channel::messages::decode_members(&ch.members)
            .into_iter()
            .map(|m| ChatChannelMember {
                peer_id: m.peer_id,
                status: m.status.into(),
            })
            .collect();
        Self {
            id: ch.id,
            name: ch.name,
            owner_id: ch.owner_id,
            members,
            version: ch.version,
            status: ch.status.into(),
            created_at: ch.created_at,
            updated_at: ch.updated_at,
        }
    }
}

#[derive(SimpleObject, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Peer {
    pub id: String,
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
    pub created_at: String,
    pub updated_at: String,
}

impl Peer {
    /// Build the GraphQL `Peer` from a `DPeer` row plus the live online flag
    /// from `PeerStatusManager`. Mirrors plain-app's
    /// `DPeer.toModel()` which calls `PeerStatusManager.isOnline(id)`.
    pub fn from_dpeer(p: DPeer, online: bool) -> Self {
        Self {
            id: p.id,
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

#[derive(SimpleObject)]
#[graphql(name = "Bookmark")]
pub struct Bookmark {
    pub id: String,
    pub url: String,
    pub title: String,
    pub favicon_path: String,
    pub group_id: String,
    pub pinned: bool,
    pub click_count: i32,
    pub last_clicked_at: Option<String>,
    pub sort_order: i32,
    pub created_at: String,
    pub updated_at: String,
}

impl From<DBookmark> for Bookmark {
    fn from(b: DBookmark) -> Self {
        Self {
            id: b.id,
            url: b.url,
            title: b.title,
            favicon_path: b.favicon_path,
            group_id: b.group_id,
            pinned: b.pinned,
            click_count: b.click_count,
            last_clicked_at: b.last_clicked_at,
            sort_order: b.sort_order,
            created_at: b.created_at,
            updated_at: b.updated_at,
        }
    }
}

#[derive(SimpleObject)]
#[graphql(name = "BookmarkGroup")]
pub struct BookmarkGroup {
    pub id: String,
    pub name: String,
    pub collapsed: bool,
    pub sort_order: i32,
    pub item_count: i32,
    pub created_at: String,
    pub updated_at: String,
}

impl BookmarkGroup {
    pub fn from_group(g: DBookmarkGroup, item_count: i32) -> Self {
        Self {
            id: g.id,
            name: g.name,
            collapsed: g.collapsed,
            sort_order: g.sort_order,
            item_count,
            created_at: g.created_at,
            updated_at: g.updated_at,
        }
    }
}

#[derive(InputObject)]
#[graphql(name = "BookmarkInput")]
pub struct BookmarkInput {
    pub url: String,
    pub title: String,
    pub group_id: String,
    pub pinned: bool,
    pub sort_order: i32,
}
