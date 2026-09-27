//! Chat-domain wire enums.
//!
//! Wire/storage representation is SCREAMING_SNAKE_CASE TEXT, matching the
//! plain-app Kotlin enums and the SQLite rows. Consumers' GraphQL layers
//! map these onto their own schema enums.

use std::fmt;
use std::str::FromStr;

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, Value, ValueRef};

// ── PeerStatus ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
