use async_graphql::{ID, InputObject, Json, SimpleObject};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(SimpleObject)]
pub struct SystemSettings {
    pub password: String,
    pub password_type: i32,
    pub auth_two_factor: bool,
    pub rotate_url_token_on_restart: bool,
    pub adb_token: String,
    pub update_info: Json<Value>,
    pub url_token: String,
    pub api_permissions: Vec<String>,
    pub onboarding_completed: bool,
    pub client_id: ID,
    pub mdns_hostname: String,
    pub signature_public_key: String,
}
impl TryFrom<plain_rs::prefs::system::SystemState> for SystemSettings {
    type Error = serde_json::Error;
    fn try_from(s: plain_rs::prefs::system::SystemState) -> Result<Self, Self::Error> {
        Ok(Self {
            password: s.password,
            password_type: s.password_type,
            auth_two_factor: s.auth_two_factor,
            rotate_url_token_on_restart: s.rotate_url_token_on_restart,
            adb_token: s.adb_token,
            update_info: Json(serde_json::to_value(s.update_info)?),
            url_token: s.url_token,
            api_permissions: s.api_permissions,
            onboarding_completed: s.onboarding_completed,
            client_id: ID(s.client_id),
            mdns_hostname: s.mdns_hostname,
            signature_public_key: s.signature_public_key,
        })
    }
}

#[derive(SimpleObject)]
pub struct UserSettings {
    pub http_port: i32,
    pub https_port: i32,
    pub dark_theme: i32,
    pub amoled_dark_theme: bool,
    pub pdf_follow_dark_theme: bool,
    pub keep_awake: bool,
    pub locale: String,
    pub service: bool,
    pub desktop_access: bool,
    pub dlna: bool,
    pub developer_mode: bool,
    pub allow_any_host: bool,
    pub device_name: String,
    pub https: bool,
    pub web_address_bar_expanded: bool,
    pub screen_mirror_quality: Json<Value>,
    pub audio_play_mode: i32,
    pub audio_playback_speed: f32,
    pub image_grid_cells_per_row: i32,
    pub video_grid_cells_per_row: i32,
    pub show_hidden_files: bool,
    pub feed_auto_refresh: bool,
    /// Feed polling interval in seconds.
    #[graphql(name = "feedAutoRefreshIntervalSec")]
    pub feed_auto_refresh_interval: i32,
    pub feed_auto_refresh_only_wifi: bool,
    pub feed_font_scale: i32,
    pub editor_wrap_content: bool,
    pub editor_font_size: i32,
    pub editor_status_bar: bool,
    pub last_file_path: Json<Value>,
    pub scan_history: Json<Value>,
    pub audio_playlist: Json<Value>,
    pub chat_input_text: String,
    pub nearby_discoverable: bool,
    pub ai_image_search_enabled: bool,
    pub notification_filter: Json<Value>,
    pub pomodoro_settings: Json<Value>,
    pub video_playlist: Json<Value>,
    pub home_features: String,
    pub quick_note_draft: String,
    pub launcher_shortcuts: String,
    pub recent_searches: Json<Value>,
    pub recent_save_dirs: Json<Value>,
    pub audio_sort_by: i32,
    pub video_sort_by: i32,
    pub image_sort_by: i32,
    pub doc_sort_by: i32,
    pub file_sort_by: i32,
    pub package_sort_by: i32,
}
impl From<plain_rs::prefs::user::UserSettings> for UserSettings {
    fn from(s: plain_rs::prefs::user::UserSettings) -> Self {
        Self {
            http_port: s.http_port,
            https_port: s.https_port,
            dark_theme: s.dark_theme,
            amoled_dark_theme: s.amoled_dark_theme,
            pdf_follow_dark_theme: s.pdf_follow_dark_theme,
            keep_awake: s.keep_awake,
            locale: s.locale,
            service: s.service,
            desktop_access: s.desktop_access,
            dlna: s.dlna,
            developer_mode: s.developer_mode,
            allow_any_host: s.allow_any_host,
            device_name: s.device_name,
            https: s.https,
            web_address_bar_expanded: s.web_address_bar_expanded,
            screen_mirror_quality: Json(s.screen_mirror_quality),
            audio_play_mode: s.audio_play_mode,
            audio_playback_speed: s.audio_playback_speed,
            image_grid_cells_per_row: s.image_grid_cells_per_row,
            video_grid_cells_per_row: s.video_grid_cells_per_row,
            show_hidden_files: s.show_hidden_files,
            feed_auto_refresh: s.feed_auto_refresh,
            feed_auto_refresh_interval: s.feed_auto_refresh_interval,
            feed_auto_refresh_only_wifi: s.feed_auto_refresh_only_wifi,
            feed_font_scale: s.feed_font_scale,
            editor_wrap_content: s.editor_wrap_content,
            editor_font_size: s.editor_font_size,
            editor_status_bar: s.editor_status_bar,
            last_file_path: Json(s.last_file_path),
            scan_history: Json(s.scan_history),
            audio_playlist: Json(s.audio_playlist),
            chat_input_text: s.chat_input_text,
            nearby_discoverable: s.nearby_discoverable,
            ai_image_search_enabled: s.ai_image_search_enabled,
            notification_filter: Json(s.notification_filter),
            pomodoro_settings: Json(s.pomodoro_settings),
            video_playlist: Json(s.video_playlist),
            home_features: s.home_features,
            quick_note_draft: s.quick_note_draft,
            launcher_shortcuts: s.launcher_shortcuts,
            recent_searches: Json(s.recent_searches),
            recent_save_dirs: Json(s.recent_save_dirs),
            audio_sort_by: s.audio_sort_by,
            video_sort_by: s.video_sort_by,
            image_sort_by: s.image_sort_by,
            doc_sort_by: s.doc_sort_by,
            file_sort_by: s.file_sort_by,
            package_sort_by: s.package_sort_by,
        }
    }
}

#[derive(InputObject, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserSettingsInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_port: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub https_port: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dark_theme: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amoled_dark_theme: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pdf_follow_dark_theme: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_awake: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop_access: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dlna: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub developer_mode: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_any_host: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub https: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_address_bar_expanded: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen_mirror_quality: Option<Json<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_play_mode: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_playback_speed: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_grid_cells_per_row: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_grid_cells_per_row: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_hidden_files: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feed_auto_refresh: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Feed polling interval in seconds.
    #[graphql(name = "feedAutoRefreshIntervalSec")]
    #[serde(rename = "feedAutoRefreshIntervalSec")]
    pub feed_auto_refresh_interval: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feed_auto_refresh_only_wifi: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feed_font_scale: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor_wrap_content: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor_font_size: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor_status_bar: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_file_path: Option<Json<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scan_history: Option<Json<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_playlist: Option<Json<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_input_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nearby_discoverable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_image_search_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification_filter: Option<Json<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pomodoro_settings: Option<Json<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_playlist: Option<Json<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home_features: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quick_note_draft: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launcher_shortcuts: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recent_searches: Option<Json<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recent_save_dirs: Option<Json<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_sort_by: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_sort_by: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_sort_by: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_sort_by: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_sort_by: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_sort_by: Option<i32>,
}

#[derive(SimpleObject)]
pub struct SettingsUpdate {
    pub result: crate::graphql::types::ActionResult,
    pub system_settings: SystemSettings,
    pub user_settings: UserSettings,
}
