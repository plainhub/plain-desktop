//! GraphQL output types for the MVP. These map 1:1 to the Go side's
//! `internal/graph/model/models_gen.go` and the SDL in
//! `internal/graph/schema.graphql`.

// Scalars + media/file/tag GraphQL types now live in plain-rs
// (`media::gql::types`) so the NAS schema and the desktop api schema
// share one definition set; the shared `DataType` wire enum lives in
// `crate::enums`.
pub use crate::enums::DataType;
pub use crate::media::gql::types::{
    ActionResult, Audio, Doc, DocExtGroup, File, FileSortBy, FileTask, FileTaskOpInput,
    FileTaskStatus, FileTaskType, Image, ImageSearchStatus, ImageSearchStatusType, Instant, Long,
    MediaBucket, MediaDataType, PathKind, ScanProgress, ScanState, Tag, TagRelation,
    TagRelationStub, TrashItem, TrashItemType, TrashSortBy, Video,
};

use async_graphql::{Enum, ID, InputObject, SimpleObject};
// ----- Custom scalars (plain-app contract) -----
//
// plain-app's schema declares `scalar Long` (signed 64-bit) and
// `scalar Instant` (RFC 3339 UTC string);
// Known gap (2026-09-20): the phone SDL declares the `MediaItem` interface
// (Audio/Image/Video/Doc implement it);
#[derive(SimpleObject, Clone, Debug)]
pub struct StorageMount {
    pub id: ID,
    pub name: String,
    pub path: String,
    pub partition_num: Option<i32>,
    pub label: Option<String>,
    pub uuid: Option<String>,
    pub mount_point: String,
    pub fs_type: String,
    pub total_bytes: Long,
    pub used_bytes: Long,
    pub free_bytes: Long,
    pub alias: String,
    pub remote: bool,
    pub drive_type: DriveType,
    /// Stable identifier for the underlying disk (OS disk uuid, foreign
    /// identifier — kept `String` per API_SPEC §8, empty when unknown).
    pub disk_id: String,
}
/// Storage bus kind of a mount (plain-app contract). `/proc/mounts` does not
/// distinguish them, so NAS mounts default to INTERNAL_STORAGE.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types, clippy::upper_case_acronyms)] // plain-app SDL contract names
pub enum DriveType {
    INTERNAL_STORAGE,
    SDCARD,
    USB_STORAGE,
    APP,
}
#[derive(SimpleObject, Clone, Debug)]
pub struct StorageDisk {
    pub id: ID,
    pub name: String,
    pub path: String,
    pub size_bytes: Long,
    pub removable: bool,
    pub model: Option<String>,
}
// ----- DeviceInfo (plain-app contract, `deviceInfo` query) -----
//
// The shared web UI's device-info page (`/developer/device-info`) selects
// plain-app's DeviceInfo shape (`deviceInfoFragment`), NOT the Go plainnas
// server-monitor shape the resolver used to serve (whose fields no client
// consumed). Mirrors plain-app `httpserver/models/DeviceInfo.kt`.

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types, clippy::upper_case_acronyms)] // plain-app SDL contract names
pub enum DevicePlatform {
    ANDROID,
    IOS,
    MACOS,
    WINDOWS,
    LINUX,
}
/// Screen geometry. `density` is the scale factor, e.g. 2.625
/// (1.0 = mdpi) — Float on the phone contract.
#[derive(SimpleObject, Clone, Debug, Default)]
pub struct DisplayInfo {
    pub width: i32,
    pub height: i32,
    pub density: f64,
}
#[derive(SimpleObject, Clone, Debug, Default)]
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
#[derive(SimpleObject, Clone, Debug)]
pub struct DeviceInfo {
    /// Device display name (stored override, falling back to hostname —
    /// same precedence as `App.deviceName`).
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
    /// Null when the platform does not expose a CPU model string.
    pub cpu_model: Option<String>,
    pub total_memory: Long,
    pub total_storage: Long,
    pub display: Option<DisplayInfo>,
    /// Only set by phone targets; a NAS reports null.
    pub android: Option<AndroidExtras>,
}
/// `deviceStatus.temperatures` item (plain-app `Temperature`).
#[derive(SimpleObject, Clone, Debug)]
pub struct Temperature {
    pub label: String,
    pub celsius: f64,
}
/// `deviceStatus` query item (plain-app `DeviceStatus`) — dynamic runtime
/// state, polled separately from the static `DeviceInfo`.
#[derive(SimpleObject, Clone, Debug, Default)]
pub struct DeviceStatus {
    pub uptime_sec: Long,
    /// 0-100; null when the device has no battery or the level is unknown.
    pub battery_level: Option<i32>,
    /// True only while actively charging; a NAS has no battery — false.
    pub charging: bool,
    /// Empty when the platform exposes no temperature sources.
    pub temperatures: Vec<Temperature>,
    /// 0-100 percent, diffed from two /proc/stat samples.
    pub cpu_usage: f64,
    pub memory_available: Option<Long>,
    /// Available bytes on the root (primary data) volume.
    pub storage_available: Long,
}
// ----- Developer pages: appLogs / dataStore / db (plain-app contract) -----

/// `dataStoreEntries` item (plain-app `KeyValuePair`).
#[derive(SimpleObject, Clone, Debug)]
pub struct KeyValuePair {
    pub key: String,
    pub value: String,
}
/// `dbTableInfo` item (plain-app `DbTableInfo`).
#[derive(SimpleObject, Clone, Debug)]
pub struct DbTableInfo {
    /// Row identity field — the table's declared primary key column
    /// (first column of a composite key).
    pub id_key: String,
}
/// Declared column type for `dbTableColumns.dataType`;UNKNOWN covers
/// undeclared columns and declared types outside SQLite's standard set
/// (plain-app `DbColumnType`).
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum DbColumnType {
    TEXT,
    INTEGER,
    REAL,
    BLOB,
    NUMERIC,
    UNKNOWN,
}
/// Map a declared SQLite column type onto `DbColumnType` (string parsing
/// lives in `crate::sqlite_browse::column_type_of`, shared with
/// plain-desktop).
pub fn column_type_of(declared: &str) -> DbColumnType {
    use crate::sqlite_browse::SqliteColumnType;
    match crate::sqlite_browse::column_type_of(declared) {
        SqliteColumnType::Text => DbColumnType::TEXT,
        SqliteColumnType::Integer => DbColumnType::INTEGER,
        SqliteColumnType::Real => DbColumnType::REAL,
        SqliteColumnType::Blob => DbColumnType::BLOB,
        SqliteColumnType::Numeric => DbColumnType::NUMERIC,
        SqliteColumnType::Unknown => DbColumnType::UNKNOWN,
    }
}
/// `dbTableColumns` item (plain-app `DbTableColumn`), straight from
/// `PRAGMA table_info`.
#[derive(SimpleObject, Clone, Debug)]
pub struct DbTableColumn {
    pub name: String,
    pub data_type: DbColumnType,
    pub not_null: bool,
    pub default_value: Option<String>,
    pub primary_key: bool,
}
// BatteryHealth / BatteryStatus / BatteryPlugged / Battery were removed:
// dynamic battery state lives in DeviceStatus (batteryLevel + charging).

#[derive(SimpleObject, Clone, Debug)]
pub struct Session {
    pub client_id: String,
    pub client_name: String,
    pub last_active: Instant,
    pub created_at: Instant,
    pub updated_at: Instant,
}
#[derive(SimpleObject, Clone, Debug)]
pub struct AuditEvent {
    pub id: ID,
    pub r#type: AuditEventType,
    pub message: String,
    pub client_id: String,
    pub created_at: Instant,
}
/// Audit-log event kinds served by the `auditEvents` query. The KV event
/// log stores snake_case kind strings;this enum is the GraphQL surface.
/// Rows with an unknown kind (written by an older build) are skipped by
/// the resolver — stale derived data is never migrated (2026-09-21 rule).
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types, clippy::upper_case_acronyms)] // SDL contract names
pub enum AuditEventType {
    LOGIN,
    LOGIN_FAILED,
    LOGOUT,
    REVOKE,
    /// OS hostname change (`setHostname`).
    SET_HOSTNAME,
    /// Display-name preference change (`updateDeviceName`).
    UPDATE_DEVICE_NAME,
    MOUNT,
    MOUNT_FAILED,
    UNMOUNT,
    FORMAT_DISK,
    FORMAT_DISK_FAILED,
}
impl AuditEventType {
    /// Map a stored kind string; `None` = unknown/retired kind.
    pub fn from_kind(kind: &str) -> Option<Self> {
        match kind {
            "login" => Some(Self::LOGIN),
            "login_failed" => Some(Self::LOGIN_FAILED),
            "logout" => Some(Self::LOGOUT),
            "revoke" => Some(Self::REVOKE),
            "set_hostname" => Some(Self::SET_HOSTNAME),
            "update_device_name" => Some(Self::UPDATE_DEVICE_NAME),
            "mount" => Some(Self::MOUNT),
            "mount_failed" => Some(Self::MOUNT_FAILED),
            "unmount" => Some(Self::UNMOUNT),
            "format_disk" => Some(Self::FORMAT_DISK),
            "format_disk_failed" => Some(Self::FORMAT_DISK_FAILED),
            _ => None,
        }
    }
}
#[derive(SimpleObject, Clone, Debug)]
pub struct AudioItem {
    pub title: String,
    pub artist: String,
    pub path: String,
    #[graphql(name = "durationMs")]
    pub duration_ms: Long,
}
/// A user playlist (plain-app `AudioPlaylist`);`itemCount` is live.
#[derive(SimpleObject, Clone, Debug)]
pub struct AudioPlaylist {
    pub id: ID,
    pub name: String,
    pub item_count: i32,
    pub created_at: Instant,
    pub updated_at: Instant,
}
/// Recently played track (plain-app `AudioPlayHistory`).
#[derive(SimpleObject, Clone, Debug)]
pub struct AudioPlayHistory {
    pub path: String,
    pub title: String,
    pub artist: String,
    #[graphql(name = "durationMs")]
    pub duration_ms: Long,
    pub play_count: Long,
    pub played_at: Instant,
}
/// Bookmark link. Mirrors the plain-app `Bookmark` model;`lastClickedAt`
/// is null until the first click is recorded.
#[derive(SimpleObject, Clone, Debug)]
pub struct Bookmark {
    pub id: ID,
    pub url: String,
    pub title: String,
    pub favicon_path: String,
    pub group_id: ID,
    pub pinned: bool,
    pub click_count: i32,
    pub last_clicked_at: Option<Instant>,
    pub sort_order: i32,
    pub created_at: Instant,
    pub updated_at: Instant,
}
/// A bookmark group;`itemCount` is the live count of bookmarks in the
/// group (0 when none).
#[derive(SimpleObject, Clone, Debug)]
pub struct BookmarkGroup {
    pub id: ID,
    pub name: String,
    pub collapsed: bool,
    pub sort_order: i32,
    pub item_count: i32,
    pub created_at: Instant,
    pub updated_at: Instant,
}
#[derive(InputObject, Clone, Debug)]
pub struct BookmarkInput {
    pub url: String,
    pub title: String,
    pub group_id: ID,
    pub pinned: bool,
    pub sort_order: i32,
}
#[derive(SimpleObject, Clone, Debug)]
pub struct App {
    /// Stable per-server identifier (auth peer identity for web clients).
    pub client_id: String,
    pub url_token: String,
    pub http_port: i32,
    pub https_port: i32,
    /// The server's app dir — for a NAS this is the data dir holding the
    /// fjall store, `prefs.json` and the on-disk log.
    pub app_dir: String,
    pub device_name: String,
    pub device_type: DeviceType,
    /// Optional capabilities this server declares; clients gate UI on
    /// these instead of sniffing OS versions (plain-app `Capability`).
    pub capabilities: Vec<Capability>,
    /// Distribution channel of this build (GitHub) — renamed from
    /// `channel` to avoid colliding with chat channels (2026-09-23).
    pub build_channel: AppChannelType,
    /// Permissions the web client's permission-gated UI keys on (plain-app
    /// contract). A NAS always has full access to its own storage, so it
    /// declares `WRITE_EXTERNAL_STORAGE` (what the media/files pages gate
    /// on); phone-only permissions have no NAS data source and stay
    /// unreported.
    pub permissions: Vec<Permission>,
    pub downloads_dir: String,
    pub developer_mode: bool,
    pub debug: bool,
}
/// Device form factor (plain-app contract);a NAS reports NAS.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types, clippy::upper_case_acronyms)] // plain-app SDL contract names
pub enum DeviceType {
    COMPUTER,
    PHONE,
    TABLET,
    TV,
    NAS,
    OTHER,
}
/// Distribution channel of the server build (plain-app contract);the NAS
/// ships from GitHub releases.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types, clippy::upper_case_acronyms)] // plain-app SDL contract names
pub enum AppChannelType {
    GITHUB,
    GOOGLE,
    FDROID,
}
/// Android runtime permissions that gate web API access (plain-app
/// contract, full member list). `App.permissions` lists the ones currently
/// enabled AND granted;a NAS always declares WRITE_EXTERNAL_STORAGE —
/// its own storage is always accessible.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types, clippy::upper_case_acronyms)] // plain-app SDL contract names
pub enum Permission {
    WRITE_EXTERNAL_STORAGE,
    READ_SMS,
    SEND_SMS,
    READ_CONTACTS,
    WRITE_CONTACTS,
    READ_CALL_LOG,
    WRITE_CALL_LOG,
    CALL_PHONE,
    POST_NOTIFICATIONS,
    NEARBY_WIFI_DEVICES,
    ACCESS_FINE_LOCATION,
    CAMERA,
    SYSTEM_ALERT_WINDOW,
    RECORD_AUDIO,
    READ_MEDIA_IMAGES,
    READ_MEDIA_VIDEOS,
    READ_MEDIA_AUDIO,
    NOTIFICATION_LISTENER,
    READ_PHONE_STATE,
    READ_PHONE_NUMBERS,
    SCHEDULE_EXACT_ALARM,
    QUERY_ALL_PACKAGES,
    ADB,
    CLIPBOARD,
}
/// Audio player state (plain-app `AudioPlayback`) — play mode preference
/// plus the current queue track. `currentPath` is null when idle. The NAS
/// server is the queue state machine;audio renders on the client, so
/// there is no server-side transport to report — `isPlaying`/`positionMs`
/// serve idle values (API_SPEC §9, unimplementable semantics = empty).
#[derive(SimpleObject, Clone, Debug)]
pub struct AudioPlayback {
    pub current_path: Option<String>,
    pub mode: MediaPlayMode,
    pub is_playing: bool,
    pub position_ms: Long,
}
/// Optional capabilities this server declares about itself (plain-app
/// `Capability` plus NAS extensions). MEDIA_TRASH is always declared:
/// a NAS implements media trash at the filesystem level (`.nas-trash`
/// tree). MEDIA_SCAN is always declared too — the scan/index engine
/// (start/pause/resume/stop + rebuild) is built in. DOC_PREVIEW is
/// declared when a LibreOffice binary is on PATH. LAN_SHARE is declared
/// when a samba systemd unit is loaded (`smbd`/`samba`/`smb`). DISK_MANAGER
/// is declared when `lsblk` is on PATH — the disk manager lists and
/// formats block devices through it.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types)] // plain-app SDL contract names
pub enum Capability {
    MEDIA_TRASH,
    MIRROR_AUDIO,
    DOC_PREVIEW,
    IMAGE_SEARCH,
    MEDIA_SCAN,
    SMS,
    CALLS,
    CALL_PHONE,
    CONTACTS,
    PACKAGES,
    NOTES,
    FEEDS,
    SCREEN_MIRROR,
    IMAGE_EDITOR,
    LAN_SHARE,
    DISK_MANAGER,
}
#[derive(SimpleObject, Clone, Debug)]
pub struct AppUpdate {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub has_update: bool,
    pub url: Option<String>,
}
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types)]
pub enum SambaShareAuth {
    GUEST,
    PASSWORD,
}
#[derive(InputObject, Clone, Debug)]
pub struct SambaShareInput {
    pub name: String,
    #[graphql(name = "sharePath")]
    pub share_path: String,
    pub auth: SambaShareAuth,
    #[graphql(name = "readOnly")]
    pub read_only: bool,
}
#[derive(InputObject, Clone, Debug)]
pub struct SambaSettingsInput {
    pub enabled: bool,
    pub shares: Vec<SambaShareInput>,
}
#[derive(SimpleObject, Clone, Debug)]
pub struct SambaShare {
    pub name: String,
    #[graphql(name = "sharePath")]
    pub share_path: String,
    pub auth: SambaShareAuth,
    #[graphql(name = "readOnly")]
    pub read_only: bool,
}
#[derive(SimpleObject, Clone, Debug)]
pub struct SambaSettings {
    pub enabled: bool,
    pub username: String,
    #[graphql(name = "hasPassword")]
    pub has_password: bool,
    pub shares: Vec<SambaShare>,
    #[graphql(name = "serviceName")]
    pub service_name: String,
    #[graphql(name = "serviceActive")]
    pub service_active: bool,
    #[graphql(name = "serviceEnabled")]
    pub service_enabled: bool,
}
#[derive(SimpleObject, Clone, Debug)]
pub struct DlnaRenderer {
    pub udn: String,
    pub name: String,
    pub manufacturer: Option<String>,
    #[graphql(name = "modelName")]
    pub model_name: Option<String>,
    pub location: String,
}
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
// The variant names mirror the Go side and the GraphQL wire protocol
// (`MediaPlayMode.REPEAT_ONE`);renaming them to `RepeatOne` would break
// every existing client.
#[allow(non_camel_case_types)]
pub enum MediaPlayMode {
    REPEAT,
    REPEAT_ONE,
    SHUFFLE,
}
#[derive(SimpleObject, Clone, Debug)]
pub struct FavoriteFolder {
    #[graphql(name = "rootPath")]
    pub root_path: String,
    #[graphql(name = "relativePath")]
    pub relative_path: String,
    /// Joined `rootPath/relativePath`, as the web client reads it.
    #[graphql(name = "fullPath")]
    pub full_path: String,
    pub alias: Option<String>,
}
// ----- Chunked-upload merge task (plain-app `MergeTask`) -----

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types)] // plain-app SDL contract names
pub enum MergeTaskStatus {
    NONE,
    STARTED,
    MERGING,
    DONE,
    FAILED,
}
/// Chunked-upload merge job state;the completion signal is the
/// upload_merge_result WS event, `mergeStatus` is the polling fallback.
#[derive(SimpleObject, Clone, Debug)]
pub struct MergeTask {
    pub status: MergeTaskStatus,
    /// Merged file name token ("name:size" split) when DONE.
    pub value: Option<String>,
    #[graphql(name = "mergedSize")]
    pub merged_size: Option<Long>,
    pub error: Option<String>,
}
// ----- Chat (plain-app contract;backed by the shared crate::chat
// stack — SQLite chat.db + LAN pairing) -----

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types)] // plain-app SDL contract names
pub enum ChatChannelStatus {
    JOINED,
    LEFT,
    KICKED,
}
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types)]
pub enum ChannelMemberStatus {
    JOINED,
    PENDING,
}
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types)]
pub enum ChatStatus {
    SENT,
    PARTIAL,
    FAILED,
    PENDING,
}
#[derive(SimpleObject, Clone, Debug)]
pub struct ChatChannelMember {
    pub peer_id: ID,
    pub status: ChannelMemberStatus,
}
#[derive(SimpleObject, Clone, Debug)]
pub struct ChatChannel {
    pub id: ID,
    pub name: String,
    pub owner_id: ID,
    pub members: Vec<ChatChannelMember>,
    pub version: Long,
    pub status: ChatChannelStatus,
    #[graphql(name = "createdAt")]
    pub created_at: Instant,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
}
#[derive(SimpleObject, Clone, Debug)]
pub struct ChatItem {
    pub id: ID,
    #[graphql(name = "fromId")]
    pub from_id: ID,
    #[graphql(name = "toId")]
    pub to_id: ID,
    /// Null for direct (peer-to-peer) messages — plain-app parity.
    #[graphql(name = "channelId")]
    pub channel_id: Option<ID>,
    pub content: String,
    #[graphql(name = "createdAt")]
    pub created_at: Instant,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
    pub status: ChatStatus,
    #[graphql(name = "statusData")]
    pub status_data: String,
}
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types)] // plain-app SDL contract names
pub enum PeerStatus {
    PAIRED,
    UNPAIRED,
    CHANNEL,
}
#[derive(SimpleObject, Clone, Debug)]
pub struct Peer {
    pub id: ID,
    pub name: String,
    pub ip: String,
    pub status: PeerStatus,
    pub port: i32,
    #[graphql(name = "deviceType")]
    pub device_type: DeviceType,
    #[graphql(name = "createdAt")]
    pub created_at: Instant,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
    pub online: bool,
}
#[derive(SimpleObject, Clone, Debug)]
pub struct AppFile {
    /// Content-addressable fileId (the fid suffix, `{sha256}[.{ext}]`).
    pub id: String,
    pub size: Long,
    #[graphql(name = "mimeType")]
    pub mime_type: String,
    #[graphql(name = "realPath")]
    pub real_path: String,
    #[graphql(name = "fileName")]
    pub file_name: String,
    #[graphql(name = "createdAt")]
    pub created_at: Instant,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
}
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types)] // plain-app SDL contract names
pub enum DiscoveryMethod {
    LAN,
    BLE,
    QR,
}
/// Initiate-pairing input (plain-app contract) — one discovered LAN device.
#[derive(InputObject, Clone, Debug)]
pub struct PairingDeviceInput {
    pub id: ID,
    pub name: String,
    pub ips: Vec<String>,
    pub port: i32,
    #[graphql(name = "deviceType")]
    pub device_type: DeviceType,
    pub version: String,
    pub platform: String,
    #[graphql(name = "lastSeen")]
    pub last_seen: Instant,
    #[graphql(name = "discoveryMethods")]
    pub discovery_methods: Vec<DiscoveryMethod>,
}
/// Incoming-pairing-request input (plain-app contract) — the wire
/// `PairingRequest` the responder received on `POST /nearby`.
#[derive(InputObject, Clone, Debug)]
pub struct PairingRequestInput {
    #[graphql(name = "fromId")]
    pub from_id: ID,
    #[graphql(name = "fromName")]
    pub from_name: String,
    pub port: i32,
    #[graphql(name = "deviceType")]
    pub device_type: DeviceType,
    #[graphql(name = "ecdhPublicKey")]
    pub ecdh_public_key: String,
    #[graphql(name = "signaturePublicKey")]
    pub signature_public_key: String,
    pub timestamp: Long,
    pub ips: Vec<String>,
    pub signature: String,
    #[graphql(name = "fromIp")]
    pub from_ip: String,
    #[graphql(name = "awareSupported")]
    pub aware_supported: bool,
}
// ── Domain → GraphQL mapping (crate::chat rows carry ISO-8601 UTC
// strings;the GraphQL layer owns the Instant/ID/scalar conversion) ──

fn iso_to_instant(s: &str) -> Instant {
    Instant(
        chrono::DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
    )
}
fn map_chat_status(s: crate::chat::enums::ChatStatus) -> ChatStatus {
    match s {
        crate::chat::enums::ChatStatus::Sent => ChatStatus::SENT,
        crate::chat::enums::ChatStatus::Partial => ChatStatus::PARTIAL,
        crate::chat::enums::ChatStatus::Failed => ChatStatus::FAILED,
        crate::chat::enums::ChatStatus::Pending => ChatStatus::PENDING,
    }
}
fn map_channel_status(s: crate::chat::enums::ChannelStatus) -> ChatChannelStatus {
    match s {
        crate::chat::enums::ChannelStatus::Joined => ChatChannelStatus::JOINED,
        crate::chat::enums::ChannelStatus::Left => ChatChannelStatus::LEFT,
        crate::chat::enums::ChannelStatus::Kicked => ChatChannelStatus::KICKED,
    }
}
fn map_member_status(s: crate::chat::enums::MemberStatus) -> ChannelMemberStatus {
    match s {
        crate::chat::enums::MemberStatus::Joined => ChannelMemberStatus::JOINED,
        crate::chat::enums::MemberStatus::Pending => ChannelMemberStatus::PENDING,
    }
}
fn map_peer_status(s: crate::chat::enums::PeerStatus) -> PeerStatus {
    match s {
        crate::chat::enums::PeerStatus::Paired => PeerStatus::PAIRED,
        crate::chat::enums::PeerStatus::Unpaired => PeerStatus::UNPAIRED,
        crate::chat::enums::PeerStatus::Channel => PeerStatus::CHANNEL,
    }
}
pub(crate) fn map_device_type(t: crate::chat::enums::DeviceType) -> DeviceType {
    match t {
        crate::chat::enums::DeviceType::Computer => DeviceType::COMPUTER,
        crate::chat::enums::DeviceType::Phone => DeviceType::PHONE,
        crate::chat::enums::DeviceType::Tablet => DeviceType::TABLET,
        crate::chat::enums::DeviceType::Tv => DeviceType::TV,
        crate::chat::enums::DeviceType::Nas => DeviceType::NAS,
        crate::chat::enums::DeviceType::Other | crate::chat::enums::DeviceType::Unknown => {
            DeviceType::OTHER
        }
    }
}
/// GraphQL DeviceType → the SCREAMING wire name the pairing protocol
/// signs and serializes.
fn device_type_wire_name(t: DeviceType) -> String {
    match t {
        DeviceType::COMPUTER => "COMPUTER",
        DeviceType::PHONE => "PHONE",
        DeviceType::TABLET => "TABLET",
        DeviceType::TV => "TV",
        DeviceType::NAS => "NAS",
        DeviceType::OTHER => "OTHER",
    }
    .to_string()
}
/// DChat row → GraphQL ChatItem. File ids are not resolved server-side —
/// clients derive them from `content` with their own urlToken.
pub(crate) fn chat_item_from_dchat(c: &crate::chat::db::DChat) -> ChatItem {
    ChatItem {
        id: ID(c.id.clone()),
        from_id: ID(c.from_id.clone()),
        to_id: ID(c.to_id.clone()),
        channel_id: if c.channel_id.is_empty() {
            None
        } else {
            Some(ID(c.channel_id.clone()))
        },
        content: c.content.clone(),
        created_at: iso_to_instant(&c.created_at),
        updated_at: iso_to_instant(&c.updated_at),
        status: map_chat_status(c.status),
        status_data: c.status_data.clone(),
    }
}
pub(crate) fn chat_channel_from_dchannel(ch: crate::chat::db::DChannel) -> ChatChannel {
    let members = crate::chat::channel::messages::decode_members(&ch.members)
        .into_iter()
        .map(|m| ChatChannelMember {
            peer_id: ID(m.peer_id),
            status: map_member_status(m.status),
        })
        .collect();
    ChatChannel {
        id: ID(ch.id),
        name: ch.name,
        owner_id: ID(ch.owner_id),
        members,
        version: Long(ch.version),
        status: map_channel_status(ch.status),
        created_at: iso_to_instant(&ch.created_at),
        updated_at: iso_to_instant(&ch.updated_at),
    }
}
pub(crate) fn peer_from_dpeer(p: crate::chat::db::DPeer, online: bool) -> Peer {
    Peer {
        id: ID(p.id),
        name: p.name,
        ip: p.ip,
        status: map_peer_status(p.status),
        port: p.port as i32,
        device_type: map_device_type(p.device_type),
        created_at: iso_to_instant(&p.created_at),
        updated_at: iso_to_instant(&p.updated_at),
        online,
    }
}
pub(crate) fn app_file_from_dappfile(f: &crate::chat::db::DAppFile, file_name: String) -> AppFile {
    AppFile {
        id: f.id.clone(),
        size: Long(f.size),
        mime_type: f.mime_type.clone(),
        real_path: f.real_path.clone(),
        file_name,
        created_at: iso_to_instant(&f.created_at),
        updated_at: iso_to_instant(&f.updated_at),
    }
}
/// PairingRequestInput (GraphQL) → the wire PairingRequest the pairing
/// manager responds to.
pub(crate) fn pairing_request_from_input(
    input: &PairingRequestInput,
) -> crate::chat::pairing::protocol::PairingRequest {
    crate::chat::pairing::protocol::PairingRequest {
        from_id: input.from_id.to_string(),
        from_name: input.from_name.clone(),
        port: input.port as u16,
        device_type: device_type_wire_name(input.device_type),
        ecdh_public_key: input.ecdh_public_key.clone(),
        signature_public_key: input.signature_public_key.clone(),
        timestamp: input.timestamp.0,
        ips: input.ips.clone(),
        signature: input.signature.clone(),
        aware_supported: input.aware_supported,
        from_ip: input.from_ip.clone(),
    }
}
// ----- FileInfo (fileInfo query) + MediaFileInfo union -----

/// Geographic coordinates (plain-app `Location`);both components non-null.
#[derive(SimpleObject, Clone, Debug)]
#[graphql(name = "Location")]
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
pub struct VideoFileInfo {
    pub width: i32,
    pub height: i32,
    #[graphql(name = "durationMs")]
    pub duration_ms: Long,
    pub location: Option<Location>,
}
#[derive(SimpleObject, Clone, Debug)]
pub struct AudioFileInfo {
    #[graphql(name = "durationMs")]
    pub duration_ms: Long,
    pub location: Option<Location>,
}
#[derive(async_graphql::Union, Clone, Debug)]
pub enum MediaFileInfo {
    Audio(AudioFileInfo),
    Image(ImageFileInfo),
    Video(VideoFileInfo),
}
#[derive(SimpleObject, Clone, Debug)]
pub struct FileInfo {
    pub path: String,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
    pub size: Long,
    pub data: Option<MediaFileInfo>,
}
#[cfg(test)]
#[path = "../../../../tests/unit/api/schema/nas/types.rs"]
mod types_tests;
