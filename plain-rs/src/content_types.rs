use async_graphql::{Enum, ID, InputObject, SimpleObject};

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

#[derive(SimpleObject, Clone, Debug)]
pub struct ActionResult {
    #[graphql(name = "affectedCount")]
    pub affected_count: i32,
}

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

pub(crate) fn parse_instant(value: &str) -> async_graphql::Result<Instant> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|dt| Instant(dt.with_timezone(&chrono::Utc)))
        .map_err(|e| async_graphql::Error::new(format!("invalid stored timestamp: {e}")))
}

#[derive(SimpleObject, Clone, Debug)]
pub struct FavoriteFolder {
    pub root_path: String,
    pub full_path: String,
    pub alias: Option<String>,
}

/// The `app` root echoes a write back rather than reading the store again,
/// so a caller can confirm what it asked for without a round trip.
#[derive(SimpleObject, Clone, Debug)]
pub struct KeyValuePair {
    pub key: String,
    pub value: String,
}

/// Optional device features. A client hides a whole section when its
/// capability is absent, which is why this is a list rather than a set of
/// booleans on the app root.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
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
}

impl Capability {
    /// Unknown members parse to `None` rather than to a capability the
    /// client would render as a section it cannot drive.
    pub(crate) fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "MEDIA_TRASH" => Self::MediaTrash,
            "MIRROR_AUDIO" => Self::MirrorAudio,
            "DOC_PREVIEW" => Self::DocPreview,
            "IMAGE_SEARCH" => Self::ImageSearch,
            "MEDIA_SCAN" => Self::MediaScan,
            "SMS" => Self::Sms,
            "CALLS" => Self::Calls,
            "CALL_PHONE" => Self::CallPhone,
            "CONTACTS" => Self::Contacts,
            "PACKAGES" => Self::Packages,
            "NOTES" => Self::Notes,
            "FEEDS" => Self::Feeds,
            "SCREEN_MIRROR" => Self::ScreenMirror,
            "IMAGE_EDITOR" => Self::ImageEditor,
            "NOTIFICATIONS" => Self::Notifications,
            "CLIPBOARD" => Self::Clipboard,
            "POMODORO" => Self::Pomodoro,
            _ => return None,
        })
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum Permission {
    WriteExternalStorage,
    ReadSms,
    SendSms,
    ReadContacts,
    WriteContacts,
    ReadCallLog,
    WriteCallLog,
    CallPhone,
    PostNotifications,
    NearbyWifiDevices,
    AccessFineLocation,
    Camera,
    SystemAlertWindow,
    RecordAudio,
    ReadMediaImages,
    ReadMediaVideos,
    ReadMediaAudio,
    NotificationListener,
    ReadPhoneState,
    ReadPhoneNumbers,
    ScheduleExactAlarm,
    QueryAllPackages,
    Adb,
    Clipboard,
}

impl Permission {
    /// Unknown members parse to `None`: a permission this build has never
    /// heard of is not one the client can request.
    pub(crate) fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "WRITE_EXTERNAL_STORAGE" => Self::WriteExternalStorage,
            "READ_SMS" => Self::ReadSms,
            "SEND_SMS" => Self::SendSms,
            "READ_CONTACTS" => Self::ReadContacts,
            "WRITE_CONTACTS" => Self::WriteContacts,
            "READ_CALL_LOG" => Self::ReadCallLog,
            "WRITE_CALL_LOG" => Self::WriteCallLog,
            "CALL_PHONE" => Self::CallPhone,
            "POST_NOTIFICATIONS" => Self::PostNotifications,
            "NEARBY_WIFI_DEVICES" => Self::NearbyWifiDevices,
            "ACCESS_FINE_LOCATION" => Self::AccessFineLocation,
            "CAMERA" => Self::Camera,
            "SYSTEM_ALERT_WINDOW" => Self::SystemAlertWindow,
            "RECORD_AUDIO" => Self::RecordAudio,
            "READ_MEDIA_IMAGES" => Self::ReadMediaImages,
            "READ_MEDIA_VIDEOS" => Self::ReadMediaVideos,
            "READ_MEDIA_AUDIO" => Self::ReadMediaAudio,
            "NOTIFICATION_LISTENER" => Self::NotificationListener,
            "READ_PHONE_STATE" => Self::ReadPhoneState,
            "READ_PHONE_NUMBERS" => Self::ReadPhoneNumbers,
            "SCHEDULE_EXACT_ALARM" => Self::ScheduleExactAlarm,
            "QUERY_ALL_PACKAGES" => Self::QueryAllPackages,
            "ADB" => Self::Adb,
            "CLIPBOARD" => Self::Clipboard,
            _ => return None,
        })
    }
}

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

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
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

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum PackageType {
    System,
    /// Anything the platform did not report as a pre-installed package.
    /// Misclassifying a user app as system would hide it from uninstall.
    #[default]
    User,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct Certificate {
    pub issuer: String,
    pub subject: String,
    #[graphql(name = "serialNumber")]
    pub serial_number: String,
    #[graphql(name = "validFrom")]
    pub valid_from: Instant,
    #[graphql(name = "validTo")]
    pub valid_to: Instant,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct Package {
    pub id: ID,
    pub name: String,
    pub r#type: PackageType,
    pub version: String,
    pub path: String,
    pub size: Long,
    pub certs: Vec<Certificate>,
    #[graphql(name = "installedAt")]
    pub installed_at: Instant,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct PackageStatus {
    pub id: ID,
    pub exists: bool,
    #[graphql(name = "updatedAt")]
    pub updated_at: Option<Instant>,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct PackageInstallPending {
    pub id: ID,
    #[graphql(name = "updatedAt")]
    pub updated_at: Option<Instant>,
    #[graphql(name = "isNew")]
    pub is_new: bool,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct Notification {
    pub id: ID,
    /// Android FLAG_ONLY_ALERT_ONCE: re-posted updates must not sound/vibrate again.
    #[graphql(name = "onlyOnce")]
    pub only_once: bool,
    #[graphql(name = "isClearable")]
    pub is_clearable: bool,
    #[graphql(name = "appId")]
    pub app_id: ID,
    #[graphql(name = "appName")]
    pub app_name: String,
    #[graphql(name = "postedAt")]
    pub posted_at: Instant,
    pub silent: bool,
    pub title: String,
    pub body: String,
    pub actions: Vec<String>,
    /// Subset of `actions` that support inline reply; `replyNotification`'s
    /// `actionIndex` indexes this list, not `actions`.
    #[graphql(name = "replyActions")]
    pub reply_actions: Vec<String>,
}
