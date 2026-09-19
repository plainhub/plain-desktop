use std::fmt;
use std::str::FromStr;

use async_graphql::Enum;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, Value, ValueRef};

// ── PeerStatus ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Enum)]
#[graphql(name = "PeerStatus", rename_items = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PeerStatus {
    Paired,
    Unpaired,
    Channel,
}

impl fmt::Display for PeerStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Paired => f.write_str("PAIRED"),
            Self::Unpaired => f.write_str("UNPAIRED"),
            Self::Channel => f.write_str("CHANNEL"),
        }
    }
}

impl FromStr for PeerStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "PAIRED" => Ok(Self::Paired),
            "UNPAIRED" => Ok(Self::Unpaired),
            "CHANNEL" => Ok(Self::Channel),
            _ => Err(format!("Unknown PeerStatus: {s}")),
        }
    }
}

impl ToSql for PeerStatus {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(Value::Text(self.to_string())))
    }
}

impl FromSql for PeerStatus {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let s = value.as_str()?;
        Self::from_str(s).map_err(|_| FromSqlError::InvalidType)
    }
}

// ── ChatStatus ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Enum)]
#[graphql(name = "ChatStatus", rename_items = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ChatStatus {
    Sent,
    Failed,
    Partial,
    Pending,
}

impl fmt::Display for ChatStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sent => f.write_str("SENT"),
            Self::Failed => f.write_str("FAILED"),
            Self::Partial => f.write_str("PARTIAL"),
            Self::Pending => f.write_str("PENDING"),
        }
    }
}

impl FromStr for ChatStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "SENT" => Ok(Self::Sent),
            "FAILED" => Ok(Self::Failed),
            "PARTIAL" => Ok(Self::Partial),
            "PENDING" => Ok(Self::Pending),
            _ => Err(format!("Unknown ChatStatus: {s}")),
        }
    }
}

impl ToSql for ChatStatus {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(Value::Text(self.to_string())))
    }
}

impl FromSql for ChatStatus {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let s = value.as_str()?;
        Self::from_str(s).map_err(|_| FromSqlError::InvalidType)
    }
}

// ── ChannelStatus ──────────────────────────────────────────────────────────

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize, Enum,
)]
#[graphql(name = "ChannelStatus", rename_items = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ChannelStatus {
    #[default]
    Joined,
    Left,
    Kicked,
}

impl fmt::Display for ChannelStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Joined => f.write_str("JOINED"),
            Self::Left => f.write_str("LEFT"),
            Self::Kicked => f.write_str("KICKED"),
        }
    }
}

impl FromStr for ChannelStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "JOINED" => Ok(Self::Joined),
            "LEFT" => Ok(Self::Left),
            "KICKED" => Ok(Self::Kicked),
            _ => Err(format!("Unknown ChannelStatus: {s}")),
        }
    }
}

impl ToSql for ChannelStatus {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(Value::Text(self.to_string())))
    }
}

impl FromSql for ChannelStatus {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let s = value.as_str()?;
        Self::from_str(s).map_err(|_| FromSqlError::InvalidType)
    }
}

// ── MemberStatus ───────────────────────────────────────────────────────────

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize, Enum,
)]
#[graphql(name = "MemberStatus", rename_items = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MemberStatus {
    #[default]
    Joined,
    Pending,
}

impl fmt::Display for MemberStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Joined => f.write_str("JOINED"),
            Self::Pending => f.write_str("PENDING"),
        }
    }
}

impl FromStr for MemberStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "JOINED" => Ok(Self::Joined),
            "PENDING" => Ok(Self::Pending),
            _ => Err(format!("Unknown MemberStatus: {s}")),
        }
    }
}

impl ToSql for MemberStatus {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(Value::Text(self.to_string())))
    }
}

impl FromSql for MemberStatus {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let s = value.as_str()?;
        Self::from_str(s).map_err(|_| FromSqlError::InvalidType)
    }
}

// ── DeviceType ─────────────────────────────────────────────────────────────

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize, Enum,
)]
#[graphql(name = "DeviceType", rename_items = "SCREAMING_SNAKE_CASE")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeviceType {
    #[default]
    Phone,
    Tablet,
    Computer,
    Tv,
    Nas,
    Other,
    Unknown,
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

impl fmt::Display for DeviceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DeviceType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "PHONE" => Ok(Self::Phone),
            "TABLET" => Ok(Self::Tablet),
            "COMPUTER" => Ok(Self::Computer),
            "TV" => Ok(Self::Tv),
            "OTHER" => Ok(Self::Other),
            "UNKNOWN" => Ok(Self::Unknown),
            _ => Err(format!("Unknown DeviceType: {s}")),
        }
    }
}

impl ToSql for DeviceType {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(Value::Text(self.to_string())))
    }
}

impl FromSql for DeviceType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let s = value.as_str()?;
        Self::from_str(s).map_err(|_| FromSqlError::InvalidType)
    }
}

// ── ChannelSystemMessageType ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Enum)]
#[graphql(
    name = "ChannelSystemMessageType",
    rename_items = "SCREAMING_SNAKE_CASE"
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ChannelSystemMessageType {
    Invite,
    InviteAccept,
    InviteDecline,
    Update,
    Kick,
    Leave,
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

impl fmt::Display for ChannelSystemMessageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ChannelSystemMessageType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "INVITE" => Ok(Self::Invite),
            "INVITE_ACCEPT" => Ok(Self::InviteAccept),
            "INVITE_DECLINE" => Ok(Self::InviteDecline),
            "UPDATE" => Ok(Self::Update),
            "KICK" => Ok(Self::Kick),
            "LEAVE" => Ok(Self::Leave),
            _ => Err(format!("Unknown ChannelSystemMessageType: {s}")),
        }
    }
}

impl ToSql for ChannelSystemMessageType {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(Value::Text(self.to_string())))
    }
}

impl FromSql for ChannelSystemMessageType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let s = value.as_str()?;
        Self::from_str(s).map_err(|_| FromSqlError::InvalidType)
    }
}

// ── ChannelSystemMessageAction ────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Enum)]
#[graphql(
    name = "ChannelSystemMessageAction",
    rename_items = "SCREAMING_SNAKE_CASE"
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ChannelSystemMessageAction {
    Invite,
    Update,
    Kick,
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

impl fmt::Display for ChannelSystemMessageAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ChannelSystemMessageAction {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "INVITE" => Ok(Self::Invite),
            "UPDATE" => Ok(Self::Update),
            "KICK" => Ok(Self::Kick),
            _ => Err(format!("Unknown ChannelSystemMessageAction: {s}")),
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

impl ToSql for DriveType {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(Value::Text(self.to_string())))
    }
}

impl FromSql for DriveType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let s = value.as_str()?;
        Self::from_str(s).map_err(|_| FromSqlError::InvalidType)
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

impl ToSql for AppChannelType {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(Value::Text(self.to_string())))
    }
}

impl FromSql for AppChannelType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let s = value.as_str()?;
        Self::from_str(s).map_err(|_| FromSqlError::InvalidType)
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

impl ToSql for SessionType {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(Value::Text(self.to_string())))
    }
}

impl FromSql for SessionType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let s = value.as_str()?;
        Self::from_str(s).map_err(|_| FromSqlError::InvalidType)
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

impl ToSql for PackageType {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(Value::Text(self.to_string())))
    }
}

impl FromSql for PackageType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let s = value.as_str()?;
        Self::from_str(s).map_err(|_| FromSqlError::InvalidType)
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

impl ToSql for DownloadStatus {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(Value::Text(self.to_string())))
    }
}

impl FromSql for DownloadStatus {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let s = value.as_str()?;
        Self::from_str(s).map_err(|_| FromSqlError::InvalidType)
    }
}
