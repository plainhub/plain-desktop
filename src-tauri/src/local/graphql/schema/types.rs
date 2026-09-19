use async_graphql::{Enum, InputObject, SimpleObject, Union};
use serde_json::Value;

use crate::local::db::{DAppFile, DBookmark, DBookmarkGroup, DChannel, DChat, DPeer};
use crate::local::enums::{
    AppChannelType, ChannelStatus, ChatStatus, DeviceType, DriveType, MemberStatus, PeerStatus,
};
use plain_rs::base64_encode;
use plain_rs::xchacha_encrypt;

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
    pub density: String,
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
    pub alias: String,
}

#[derive(SimpleObject)]
pub struct KeyValuePair {
    pub key: String,
    pub value: String,
}

/// Audio player state (plain-app `AudioPlayback`). The desktop backend has
/// no playback engine, so the resolver serves inert defaults.
#[derive(SimpleObject)]
#[graphql(name = "AudioPlayback")]
pub struct AudioPlayback {
    pub mode: String,
    pub current_path: String,
}

/// Optional capabilities the server declares about itself; the web client
/// gates UI on these instead of sniffing OS versions. Mirrors plain-app
/// `DeviceFeature`.
#[derive(Enum, Copy, Clone, Eq, PartialEq)]
#[graphql(name = "DeviceFeature", rename_items = "SCREAMING_SNAKE_CASE")]
pub enum DeviceFeature {
    MediaTrash,
    MirrorAudio,
}

#[derive(SimpleObject)]
pub struct App {
    pub client_id: String,
    pub usb_connected: bool,
    pub url_token: String,
    pub http_port: i32,
    pub https_port: i32,
    pub app_dir: String,
    pub device_name: String,
    pub device_type: DeviceType,
    pub features: Vec<DeviceFeature>,
    pub channel: AppChannelType,
    pub permissions: Vec<String>,
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
    #[graphql(name = "diskID")]
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
    /// Seconds. `0` in local mode — the desktop local server has no
    /// `MediaMetadataRetriever` equivalent. Main-window traffic still goes
    /// through the device server and returns real durations.
    pub duration: i64,
    pub location: Option<Location>,
}

#[derive(SimpleObject)]
pub struct AudioFileInfo {
    /// Seconds. `0` in local mode (see `VideoFileInfo::duration`).
    pub duration: i64,
    pub location: Option<Location>,
}

/// Polymorphic media-metadata payload — clients select via `... on XFileInfo`.
#[derive(Union)]
pub enum MediaFileInfo {
    Image(ImageFileInfo),
    Video(VideoFileInfo),
    Audio(AudioFileInfo),
}

/// `fileInfo` query result. `data` is `None` for non-media files; `tags` is
/// empty in local mode (plain-web doesn't yet persist tag relations).
#[derive(SimpleObject)]
pub struct FileInfo {
    pub path: String,
    pub updated_at: String,
    pub size: i64,
    pub tags: Vec<Tag>,
    pub data: Option<MediaFileInfo>,
}

/// Mirrors plain-app `web/models/Tag.kt`. The local server returns an
/// empty list — the `tags` query exists purely to satisfy schema
/// resolution for the popup lightbox.
#[derive(SimpleObject, Clone)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub count: i32,
}

/// Union for `ChatItem.data` — always `None` in local mode but schema must match the fragment.
#[derive(Union)]
#[allow(clippy::enum_variant_names)]
pub enum ChatItemData {
    MessageImages(MessageImages),
    MessageFiles(MessageFiles),
    MessageText(MessageText),
}

#[derive(SimpleObject)]
pub struct MessageImages {
    pub ids: Vec<String>,
}

#[derive(SimpleObject)]
pub struct MessageFiles {
    pub ids: Vec<String>,
}

#[derive(SimpleObject)]
pub struct MessageText {
    pub ids: Vec<String>,
}

#[derive(SimpleObject)]
pub struct ChatItem {
    pub id: String,
    pub from_id: String,
    pub to_id: String,
    pub channel_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub content: String,
    pub status: ChatStatus,
    pub status_data: String,
    pub data: Option<ChatItemData>,
}

impl From<DChat> for ChatItem {
    fn from(c: DChat) -> Self {
        Self {
            id: c.id,
            from_id: c.from_id,
            to_id: c.to_id,
            channel_id: c.channel_id,
            created_at: c.created_at,
            updated_at: c.updated_at,
            content: c.content,
            status: c.status,
            status_data: c.status_data,
            data: None,
        }
    }
}

impl ChatItem {
    /// Build a `ChatItem` with its `data` field populated. The data
    /// field's `ids` are the URL-token-encrypted form expected by the
    /// `/fs` endpoint — see `plain-app` `FileHelper.getFileId` for the
    /// Kotlin reference.
    pub(crate) fn with_data(c: DChat, token: &str) -> Self {
        let mut item = Self::from(c);
        item.data = chat_item_data_from_content(&item.content, token);
        item
    }
}

pub(crate) fn chat_item_data_from_content(content: &str, token: &str) -> Option<ChatItemData> {
    let v: Value = serde_json::from_str(content).ok()?;
    let msg_type = v.get("type")?.as_str()?;
    let value = v.get("value")?;
    match msg_type {
        "IMAGES" => {
            let ids = value
                .get("items")?
                .as_array()?
                .iter()
                .filter_map(|i| {
                    let uri = i.get("uri").and_then(|u| u.as_str())?;
                    let name = i.get("fileName").and_then(|u| u.as_str()).unwrap_or("");
                    Some(make_file_id_json(uri, name, token))
                })
                .collect();
            Some(ChatItemData::MessageImages(MessageImages { ids }))
        }
        "FILES" => {
            let ids = value
                .get("items")?
                .as_array()?
                .iter()
                .filter_map(|i| {
                    let uri = i.get("uri").and_then(|u| u.as_str())?;
                    let name = i.get("fileName").and_then(|u| u.as_str()).unwrap_or("");
                    Some(make_file_id_json(uri, name, token))
                })
                .collect();
            Some(ChatItemData::MessageFiles(MessageFiles { ids }))
        }
        "TEXT" => {
            // For text messages, the encryption input is the bare
            // `imageLocalPath` (not wrapped in JSON) — matches plain-app
            // `ChatItem.getContentData()`'s `MessageText` branch.
            let ids = value
                .get("linkPreviews")?
                .as_array()?
                .iter()
                .filter_map(|p| p.get("imageLocalPath").and_then(|s| s.as_str()))
                .filter(|s| !s.is_empty())
                .map(|p| make_file_id(p, token))
                .collect();
            Some(ChatItemData::MessageText(MessageText { ids }))
        }
        _ => None,
    }
}

/// Encrypt `JSON.stringify({path, name})` with the local URL token and
/// base64-encode the result. Mirrors `plain-app`'s
/// `FileHelper.getFileId(JSONObject().apply { put("path", …); put("name", …) })`
/// for chat image/file payloads.
pub(crate) fn make_file_id_json(path: &str, name: &str, token: &str) -> String {
    let json = serde_json::json!({ "path": path, "name": name }).to_string();
    make_file_id(&json, token)
}

/// Encrypt `path` with the local URL token and base64-encode the
/// result. Mirrors `plain-app`'s `FileHelper.getFileId(path)` for the
/// text-message link-preview path (no JSON wrapping).
pub(crate) fn make_file_id(path: &str, token: &str) -> String {
    let Some(encrypted) = xchacha_encrypt(token, path.as_bytes()) else {
        return String::new();
    };
    base64_encode(&encrypted)
}

#[derive(SimpleObject, Clone)]
pub struct ChatChannelMember {
    pub id: String,
    pub status: MemberStatus,
}

#[derive(SimpleObject)]
pub struct ChatChannel {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub members: Vec<ChatChannelMember>,
    pub version: i64,
    pub status: ChannelStatus,
    pub created_at: String,
    pub updated_at: String,
}

impl From<DChannel> for ChatChannel {
    fn from(ch: DChannel) -> Self {
        let members = crate::local::channel::messages::decode_members(&ch.members)
            .into_iter()
            .map(|m| ChatChannelMember {
                id: m.id,
                status: m.status,
            })
            .collect();
        Self {
            id: ch.id,
            name: ch.name,
            owner: ch.owner,
            members,
            version: ch.version,
            status: ch.status,
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
            status: p.status,
            online,
            port: p.port as i32,
            device_type: p.device_type,
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
    pub created_at: String,
    pub updated_at: String,
}

impl From<DBookmarkGroup> for BookmarkGroup {
    fn from(g: DBookmarkGroup) -> Self {
        Self {
            id: g.id,
            name: g.name,
            collapsed: g.collapsed,
            sort_order: g.sort_order,
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

#[derive(SimpleObject)]
#[graphql(name = "AppFile")]
pub struct AppFile {
    pub id: String,
    pub size: i64,
    pub mime_type: String,
    pub real_path: String,
    pub file_name: String,
    pub created_at: String,
    pub updated_at: String,
}

impl AppFile {
    pub fn from_dappfile(f: DAppFile, file_name: String) -> Self {
        Self {
            id: f.id,
            size: f.size,
            mime_type: f.mime_type,
            real_path: f.real_path,
            file_name,
            created_at: f.created_at,
            updated_at: f.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plain_rs::xchacha_decrypt;
    use plain_rs::{base64_decode, base64_encode};

    /// Roundtrip the `make_file_id_json` output through the same
    /// decryption the `/fs` handler does. This is the contract that
    /// keeps chat images loading — if this test fails, the server is
    /// either encrypting the wrong thing or the token doesn't match
    /// the one the web side will derive from `app.urlToken`.
    #[test]
    fn make_file_id_json_roundtrips_through_fs_decrypt() {
        let token_raw = [42u8; 32];
        let token_b64 = base64_encode(&token_raw);

        let fid = make_file_id_json("fid:abc123def456.jpg", "cat.jpg", &token_b64);
        assert!(!fid.is_empty(), "encrypted id should not be empty");

        // `/fs` handler: base64-decode the query param, then
        // xchacha_decrypt with the local URL token.
        let encrypted = base64_decode(&fid);
        let plaintext = xchacha_decrypt(&token_b64, &encrypted)
            .expect("server-side encrypted id must decrypt with local token");

        // The decrypted plaintext is the JSON we built in
        // `make_file_id_json` — must round-trip to the original
        // `{path, name}` pair, which is what `file_server` then
        // passes to `resolve_uri`.
        let v: serde_json::Value = serde_json::from_slice(&plaintext).unwrap();
        assert_eq!(v["path"], "fid:abc123def456.jpg");
        assert_eq!(v["name"], "cat.jpg");
    }

    /// `make_file_id` (the `&str` variant) is used for text-message
    /// link-preview paths. Decryption should give back the bare
    /// `imageLocalPath` (not JSON-wrapped).
    #[test]
    fn make_file_id_roundtrips_through_fs_decrypt() {
        let token_raw = [7u8; 32];
        let token_b64 = base64_encode(&token_raw);

        let path = "app://Pictures/foo.png";
        let fid = make_file_id(path, &token_b64);

        let plaintext = xchacha_decrypt(&token_b64, &base64_decode(&fid))
            .expect("decrypt must succeed for text link-preview path");
        assert_eq!(std::str::from_utf8(&plaintext).unwrap(), path);
    }

    /// The full `chat_item_data_from_content` flow: build a chat
    /// `content` JSON, compute its `data` with the token, decrypt
    /// each id back, and confirm we recover the original `{path,
    /// name}`.
    #[test]
    fn chat_item_data_from_content_roundtrips() {
        let token_b64 = base64_encode(&[99u8; 32]);

        let content = serde_json::json!({
            "type": "IMAGES",
            "value": {
                "items": [
                    { "uri": "fid:00112233.jpg", "fileName": "first.jpg" },
                    { "uri": "fid:ffeeddcc.png", "fileName": "second.png" },
                ]
            }
        })
        .to_string();

        let data = chat_item_data_from_content(&content, &token_b64).expect("should parse content");
        let ids = match data {
            ChatItemData::MessageImages(m) => m.ids,
            _ => panic!("expected MessageImages"),
        };
        assert_eq!(ids.len(), 2);

        for (i, expected) in ["fid:00112233.jpg", "fid:ffeeddcc.png"].iter().enumerate() {
            let expected_name = if i == 0 { "first.jpg" } else { "second.png" };
            let plaintext =
                xchacha_decrypt(&token_b64, &base64_decode(&ids[i])).expect("must decrypt");
            let v: serde_json::Value = serde_json::from_slice(&plaintext).unwrap();
            assert_eq!(v["path"], *expected);
            assert_eq!(v["name"], expected_name);
        }
    }
}
