//! GraphQL-facing enums. The chat-domain enums mirror the wire enums in
//! `plain_rs::chat::enums` (which own the SQLite/wire representation);
//! the `From` impls below convert what the shared store returns into
//! these GraphQL types. The remaining enums are desktop-only surfaces.

use std::fmt;
use std::str::FromStr;

use async_graphql::Enum;

macro_rules! wire_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $wire:expr),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Enum)]
        #[graphql(rename_items = "SCREAMING_SNAKE_CASE")]
        #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
        pub enum $name {
            $($variant,)+
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    $(Self::$variant => f.write_str($wire),)+
                }
            }
        }

        impl FromStr for $name {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $($wire => Ok(Self::$variant),)+
                    _ => Err(format!("Unknown {}: {s}", stringify!($name))),
                }
            }
        }

        impl From<plain_rs::chat::enums::$name> for $name {
            fn from(v: plain_rs::chat::enums::$name) -> Self {
                match v {
                    $(plain_rs::chat::enums::$name::$variant => Self::$variant,)+
                }
            }
        }

        impl From<$name> for plain_rs::chat::enums::$name {
            fn from(v: $name) -> Self {
                match v {
                    $($name::$variant => plain_rs::chat::enums::$name::$variant,)+
                }
            }
        }
    };
}

// Pairing state of a stored peer.
wire_enum! {
    PeerStatus {
        Paired => "PAIRED",
        Unpaired => "UNPAIRED",
        Channel => "CHANNEL",
    }
}

// Delivery state of a chat item.
wire_enum! {
    ChatStatus {
        Sent => "SENT",
        Failed => "FAILED",
        Partial => "PARTIAL",
        Pending => "PENDING",
    }
}

// Membership state of the local device in a channel.
wire_enum! {
    ChannelStatus {
        Joined => "JOINED",
        Left => "LEFT",
        Kicked => "KICKED",
    }
}

// Membership state of a channel member.
wire_enum! {
    MemberStatus {
        Joined => "JOINED",
        Pending => "PENDING",
    }
}

// Device class advertised on the wire.
wire_enum! {
    DeviceType {
        Phone => "PHONE",
        Tablet => "TABLET",
        Computer => "COMPUTER",
        Tv => "TV",
        Nas => "NAS",
        Other => "OTHER",
        Unknown => "UNKNOWN",
    }
}

// Channel system-message wire types.
wire_enum! {
    ChannelSystemMessageType {
        Invite => "INVITE",
        InviteAccept => "INVITE_ACCEPT",
        InviteDecline => "INVITE_DECLINE",
        Update => "UPDATE",
        Kick => "KICK",
        Leave => "LEAVE",
    }
}

// Channel system-message actions.
wire_enum! {
    ChannelSystemMessageAction {
        Invite => "INVITE",
        Update => "UPDATE",
        Kick => "KICK",
    }
}

#[allow(clippy::derivable_impls)]
impl Default for ChannelStatus {
    fn default() -> Self {
        Self::Joined
    }
}

#[allow(clippy::derivable_impls)]
impl Default for MemberStatus {
    fn default() -> Self {
        Self::Joined
    }
}

#[allow(clippy::derivable_impls)]
impl Default for DeviceType {
    fn default() -> Self {
        Self::Phone
    }
}

impl DeviceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Phone => "PHONE",
            Self::Tablet => "TABLET",
            Self::Computer => "COMPUTER",
            Self::Tv => "TV",
            Self::Nas => "NAS",
            Self::Other => "OTHER",
            Self::Unknown => "UNKNOWN",
        }
    }
}

impl ChannelSystemMessageType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Invite => "INVITE",
            Self::InviteAccept => "INVITE_ACCEPT",
            Self::InviteDecline => "INVITE_DECLINE",
            Self::Update => "UPDATE",
            Self::Kick => "KICK",
            Self::Leave => "LEAVE",
        }
    }
}

impl ChannelSystemMessageAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Invite => "INVITE",
            Self::Update => "UPDATE",
            Self::Kick => "KICK",
        }
    }
}

// ── DriveType ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Enum)]
#[graphql(name = "DriveType", rename_items = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DriveType {
    InternalStorage,
    Sdcard,
    UsbStorage,
    App,
}

impl DriveType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InternalStorage => "INTERNAL_STORAGE",
            Self::Sdcard => "SDCARD",
            Self::UsbStorage => "USB_STORAGE",
            Self::App => "APP",
        }
    }
}

impl fmt::Display for DriveType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DriveType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "INTERNAL_STORAGE" => Ok(Self::InternalStorage),
            "SDCARD" => Ok(Self::Sdcard),
            "USB_STORAGE" => Ok(Self::UsbStorage),
            "APP" => Ok(Self::App),
            _ => Err(format!("Unknown DriveType: {s}")),
        }
    }
}

// ── AppChannelType ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Enum)]
#[graphql(name = "AppChannelType", rename_items = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppChannelType {
    Github,
    Google,
    Fdroid,
}

impl AppChannelType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Github => "GITHUB",
            Self::Google => "GOOGLE",
            Self::Fdroid => "FDROID",
        }
    }
}

impl fmt::Display for AppChannelType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AppChannelType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "GITHUB" => Ok(Self::Github),
            "GOOGLE" => Ok(Self::Google),
            "FDROID" => Ok(Self::Fdroid),
            _ => Err(format!("Unknown AppChannelType: {s}")),
        }
    }
}

// ── SessionType ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Enum)]
#[graphql(name = "SessionType", rename_items = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SessionType {
    Web,
    Custom,
}

impl SessionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Web => "WEB",
            Self::Custom => "CUSTOM",
        }
    }
}

impl fmt::Display for SessionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SessionType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "WEB" => Ok(Self::Web),
            "CUSTOM" => Ok(Self::Custom),
            _ => Err(format!("Unknown SessionType: {s}")),
        }
    }
}

// ── PackageType ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Enum)]
#[graphql(name = "PackageType", rename_items = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PackageType {
    System,
    User,
}

impl PackageType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::System => "SYSTEM",
            Self::User => "USER",
        }
    }
}

impl fmt::Display for PackageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for PackageType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "SYSTEM" => Ok(Self::System),
            "USER" => Ok(Self::User),
            _ => Err(format!("Unknown PackageType: {s}")),
        }
    }
}

// ── DownloadStatus ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Enum)]
#[graphql(name = "DownloadStatus", rename_items = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DownloadStatus {
    Pending,
    Downloading,
    Paused,
    Completed,
    Failed,
    Canceled,
}

impl DownloadStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Downloading => "DOWNLOADING",
            Self::Paused => "PAUSED",
            Self::Completed => "COMPLETED",
            Self::Failed => "FAILED",
            Self::Canceled => "CANCELED",
        }
    }
}

impl fmt::Display for DownloadStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DownloadStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "PENDING" => Ok(Self::Pending),
            "DOWNLOADING" => Ok(Self::Downloading),
            "PAUSED" => Ok(Self::Paused),
            "COMPLETED" => Ok(Self::Completed),
            "FAILED" => Ok(Self::Failed),
            "CANCELED" => Ok(Self::Canceled),
            _ => Err(format!("Unknown DownloadStatus: {s}")),
        }
    }
}

// ── DataType ───────────────────────────────────────────────────────────────

/// Taggable data domains, mirroring the plain-app `DataType` wire enum.
/// GraphQL-arg only — the local server has no tag tables yet; it exists so
/// enum-typed variables in `tags(type: $type)` documents validate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Enum)]
#[graphql(name = "DataType", rename_items = "SCREAMING_SNAKE_CASE")]
pub enum DataType {
    Default,
    Audio,
    Video,
    Image,
    Sms,
    Contact,
    Note,
    FeedEntry,
    Call,
    Package,
    File,
    AppFile,
    Doc,
}

// ── MediaPlayMode ──────────────────────────────────────────────────────────

/// Audio playback repeat mode, mirroring the plain-app `MediaPlayMode` enum.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Enum)]
#[graphql(name = "MediaPlayMode", rename_items = "SCREAMING_SNAKE_CASE")]
pub enum MediaPlayMode {
    Repeat,
    RepeatOne,
    Shuffle,
}

// ── Permission ─────────────────────────────────────────────────────────────

/// Client-side access rights, mirroring the plain-app `Permission` enum.
/// The desktop local server grants none — phone features require a paired
/// phone backend.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Enum)]
#[graphql(name = "Permission", rename_items = "SCREAMING_SNAKE_CASE")]
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
