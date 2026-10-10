mod actions;
pub mod keys;
mod patch;
mod state;
mod validation;
use super::{Prefs, PrefsError, Result};
pub use actions::Action;
pub use patch::UserSettingsPatch;
use serde_json::{Map, Value};
pub use state::UserSettings;
use validation::validate;

pub fn snapshot(prefs: &Prefs) -> Result<UserSettings> {
    let entries: Map<String, Value> = prefs.user_entries().into_iter().collect();
    let mut settings = UserSettings::default();
    if let Some(value) = entries.get(keys::HTTP_PORT) {
        settings.http_port = serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::HTTPS_PORT) {
        settings.https_port = serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::DARK_THEME) {
        settings.dark_theme = serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::AMOLED_DARK_THEME) {
        settings.amoled_dark_theme =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::PDF_FOLLOW_DARK_THEME) {
        settings.pdf_follow_dark_theme =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::KEEP_AWAKE) {
        settings.keep_awake = serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::LOCALE) {
        settings.locale = serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::SERVICE) {
        settings.service = serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::DESKTOP_ACCESS) {
        settings.desktop_access =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::DLNA) {
        settings.dlna = serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::DEVELOPER_MODE) {
        settings.developer_mode =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::ALLOW_ANY_HOST) {
        settings.allow_any_host =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::DEVICE_NAME) {
        settings.device_name = serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::HTTPS) {
        settings.https = serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::WEB_ADDRESS_BAR_EXPANDED) {
        settings.web_address_bar_expanded =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::SCREEN_MIRROR_QUALITY) {
        settings.screen_mirror_quality =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::AUDIO_PLAY_MODE) {
        settings.audio_play_mode =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::AUDIO_PLAYBACK_SPEED) {
        settings.audio_playback_speed =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::IMAGE_GRID_CELLS_PER_ROW) {
        settings.image_grid_cells_per_row =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::VIDEO_GRID_CELLS_PER_ROW) {
        settings.video_grid_cells_per_row =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::SHOW_HIDDEN_FILES) {
        settings.show_hidden_files =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::FEED_AUTO_REFRESH) {
        settings.feed_auto_refresh =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::FEED_AUTO_REFRESH_INTERVAL) {
        settings.feed_auto_refresh_interval =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::FEED_AUTO_REFRESH_ONLY_WIFI) {
        settings.feed_auto_refresh_only_wifi =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::FEED_FONT_SCALE) {
        settings.feed_font_scale =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::EDITOR_WRAP_CONTENT) {
        settings.editor_wrap_content =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::EDITOR_FONT_SIZE) {
        settings.editor_font_size =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::EDITOR_STATUS_BAR) {
        settings.editor_status_bar =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::LAST_FILE_PATH) {
        settings.last_file_path =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::SCAN_HISTORY) {
        settings.scan_history =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::AUDIO_PLAYLIST) {
        settings.audio_playlist =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::CHAT_INPUT_TEXT) {
        settings.chat_input_text =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::NEARBY_DISCOVERABLE) {
        settings.nearby_discoverable =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::AI_IMAGE_SEARCH_ENABLED) {
        settings.ai_image_search_enabled =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::NOTIFICATION_FILTER) {
        settings.notification_filter =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::POMODORO_SETTINGS) {
        settings.pomodoro_settings =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::VIDEO_PLAYLIST) {
        settings.video_playlist =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::HOME_FEATURES) {
        settings.home_features =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::QUICK_NOTE_DRAFT) {
        settings.quick_note_draft =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::LAUNCHER_SHORTCUTS) {
        settings.launcher_shortcuts =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::RECENT_SEARCHES) {
        settings.recent_searches =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::RECENT_SAVE_DIRS) {
        settings.recent_save_dirs =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::AUDIO_SORT_BY) {
        settings.audio_sort_by =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::VIDEO_SORT_BY) {
        settings.video_sort_by =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::IMAGE_SORT_BY) {
        settings.image_sort_by =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::DOC_SORT_BY) {
        settings.doc_sort_by = serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::FILE_SORT_BY) {
        settings.file_sort_by =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    if let Some(value) = entries.get(keys::PACKAGE_SORT_BY) {
        settings.package_sort_by =
            serde_json::from_value(value.clone()).map_err(PrefsError::Decode)?;
    }
    Ok(settings)
}

pub fn patch(prefs: &Prefs, patch: UserSettingsPatch) -> anyhow::Result<bool> {
    validate(&patch)?;
    let mut changed = false;
    prefs.update_user(|entries| {
        if let Some(value) = patch.http_port {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::HTTP_PORT) != Some(&value) {
                entries.insert(keys::HTTP_PORT.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.https_port {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::HTTPS_PORT) != Some(&value) {
                entries.insert(keys::HTTPS_PORT.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.dark_theme {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::DARK_THEME) != Some(&value) {
                entries.insert(keys::DARK_THEME.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.amoled_dark_theme {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::AMOLED_DARK_THEME) != Some(&value) {
                entries.insert(keys::AMOLED_DARK_THEME.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.pdf_follow_dark_theme {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::PDF_FOLLOW_DARK_THEME) != Some(&value) {
                entries.insert(keys::PDF_FOLLOW_DARK_THEME.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.keep_awake {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::KEEP_AWAKE) != Some(&value) {
                entries.insert(keys::KEEP_AWAKE.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.locale {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::LOCALE) != Some(&value) {
                entries.insert(keys::LOCALE.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.service {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::SERVICE) != Some(&value) {
                entries.insert(keys::SERVICE.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.desktop_access {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::DESKTOP_ACCESS) != Some(&value) {
                entries.insert(keys::DESKTOP_ACCESS.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.dlna {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::DLNA) != Some(&value) {
                entries.insert(keys::DLNA.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.developer_mode {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::DEVELOPER_MODE) != Some(&value) {
                entries.insert(keys::DEVELOPER_MODE.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.allow_any_host {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::ALLOW_ANY_HOST) != Some(&value) {
                entries.insert(keys::ALLOW_ANY_HOST.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.device_name {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::DEVICE_NAME) != Some(&value) {
                entries.insert(keys::DEVICE_NAME.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.https {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::HTTPS) != Some(&value) {
                entries.insert(keys::HTTPS.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.web_address_bar_expanded {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::WEB_ADDRESS_BAR_EXPANDED) != Some(&value) {
                entries.insert(keys::WEB_ADDRESS_BAR_EXPANDED.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.screen_mirror_quality {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::SCREEN_MIRROR_QUALITY) != Some(&value) {
                entries.insert(keys::SCREEN_MIRROR_QUALITY.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.audio_play_mode {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::AUDIO_PLAY_MODE) != Some(&value) {
                entries.insert(keys::AUDIO_PLAY_MODE.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.audio_playback_speed {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::AUDIO_PLAYBACK_SPEED) != Some(&value) {
                entries.insert(keys::AUDIO_PLAYBACK_SPEED.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.image_grid_cells_per_row {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::IMAGE_GRID_CELLS_PER_ROW) != Some(&value) {
                entries.insert(keys::IMAGE_GRID_CELLS_PER_ROW.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.video_grid_cells_per_row {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::VIDEO_GRID_CELLS_PER_ROW) != Some(&value) {
                entries.insert(keys::VIDEO_GRID_CELLS_PER_ROW.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.show_hidden_files {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::SHOW_HIDDEN_FILES) != Some(&value) {
                entries.insert(keys::SHOW_HIDDEN_FILES.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.feed_auto_refresh {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::FEED_AUTO_REFRESH) != Some(&value) {
                entries.insert(keys::FEED_AUTO_REFRESH.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.feed_auto_refresh_interval {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::FEED_AUTO_REFRESH_INTERVAL) != Some(&value) {
                entries.insert(keys::FEED_AUTO_REFRESH_INTERVAL.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.feed_auto_refresh_only_wifi {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::FEED_AUTO_REFRESH_ONLY_WIFI) != Some(&value) {
                entries.insert(keys::FEED_AUTO_REFRESH_ONLY_WIFI.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.feed_font_scale {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::FEED_FONT_SCALE) != Some(&value) {
                entries.insert(keys::FEED_FONT_SCALE.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.editor_wrap_content {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::EDITOR_WRAP_CONTENT) != Some(&value) {
                entries.insert(keys::EDITOR_WRAP_CONTENT.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.editor_font_size {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::EDITOR_FONT_SIZE) != Some(&value) {
                entries.insert(keys::EDITOR_FONT_SIZE.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.editor_status_bar {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::EDITOR_STATUS_BAR) != Some(&value) {
                entries.insert(keys::EDITOR_STATUS_BAR.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.last_file_path {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::LAST_FILE_PATH) != Some(&value) {
                entries.insert(keys::LAST_FILE_PATH.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.scan_history {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::SCAN_HISTORY) != Some(&value) {
                entries.insert(keys::SCAN_HISTORY.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.audio_playlist {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::AUDIO_PLAYLIST) != Some(&value) {
                entries.insert(keys::AUDIO_PLAYLIST.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.chat_input_text {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::CHAT_INPUT_TEXT) != Some(&value) {
                entries.insert(keys::CHAT_INPUT_TEXT.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.nearby_discoverable {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::NEARBY_DISCOVERABLE) != Some(&value) {
                entries.insert(keys::NEARBY_DISCOVERABLE.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.ai_image_search_enabled {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::AI_IMAGE_SEARCH_ENABLED) != Some(&value) {
                entries.insert(keys::AI_IMAGE_SEARCH_ENABLED.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.notification_filter {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::NOTIFICATION_FILTER) != Some(&value) {
                entries.insert(keys::NOTIFICATION_FILTER.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.pomodoro_settings {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::POMODORO_SETTINGS) != Some(&value) {
                entries.insert(keys::POMODORO_SETTINGS.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.video_playlist {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::VIDEO_PLAYLIST) != Some(&value) {
                entries.insert(keys::VIDEO_PLAYLIST.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.home_features {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::HOME_FEATURES) != Some(&value) {
                entries.insert(keys::HOME_FEATURES.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.quick_note_draft {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::QUICK_NOTE_DRAFT) != Some(&value) {
                entries.insert(keys::QUICK_NOTE_DRAFT.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.launcher_shortcuts {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::LAUNCHER_SHORTCUTS) != Some(&value) {
                entries.insert(keys::LAUNCHER_SHORTCUTS.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.recent_searches {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::RECENT_SEARCHES) != Some(&value) {
                entries.insert(keys::RECENT_SEARCHES.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.recent_save_dirs {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::RECENT_SAVE_DIRS) != Some(&value) {
                entries.insert(keys::RECENT_SAVE_DIRS.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.audio_sort_by {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::AUDIO_SORT_BY) != Some(&value) {
                entries.insert(keys::AUDIO_SORT_BY.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.video_sort_by {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::VIDEO_SORT_BY) != Some(&value) {
                entries.insert(keys::VIDEO_SORT_BY.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.image_sort_by {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::IMAGE_SORT_BY) != Some(&value) {
                entries.insert(keys::IMAGE_SORT_BY.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.doc_sort_by {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::DOC_SORT_BY) != Some(&value) {
                entries.insert(keys::DOC_SORT_BY.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.file_sort_by {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::FILE_SORT_BY) != Some(&value) {
                entries.insert(keys::FILE_SORT_BY.into(), value);
                changed = true;
            }
        }
        if let Some(value) = patch.package_sort_by {
            let value = serde_json::to_value(value).map_err(PrefsError::Encode)?;
            if entries.get(keys::PACKAGE_SORT_BY) != Some(&value) {
                entries.insert(keys::PACKAGE_SORT_BY.into(), value);
                changed = true;
            }
        }
        Ok(())
    })?;
    Ok(changed)
}
#[cfg(test)]
#[path = "../../../tests/unit/prefs/user.rs"]
mod tests;

pub fn set_entry(prefs: &Prefs, key: &str, value: Value) -> anyhow::Result<Option<bool>> {
    let field = match key {
        keys::HTTP_PORT => "httpPort",
        keys::HTTPS_PORT => "httpsPort",
        keys::DARK_THEME => "darkTheme",
        keys::AMOLED_DARK_THEME => "amoledDarkTheme",
        keys::PDF_FOLLOW_DARK_THEME => "pdfFollowDarkTheme",
        keys::KEEP_AWAKE => "keepAwake",
        keys::LOCALE => "locale",
        keys::SERVICE => "service",
        keys::DESKTOP_ACCESS => "desktopAccess",
        keys::DLNA => "dlna",
        keys::DEVELOPER_MODE => "developerMode",
        keys::ALLOW_ANY_HOST => "allowAnyHost",
        keys::DEVICE_NAME => "deviceName",
        keys::HTTPS => "https",
        keys::WEB_ADDRESS_BAR_EXPANDED => "webAddressBarExpanded",
        keys::SCREEN_MIRROR_QUALITY => "screenMirrorQuality",
        keys::AUDIO_PLAY_MODE => "audioPlayMode",
        keys::AUDIO_PLAYBACK_SPEED => "audioPlaybackSpeed",
        keys::IMAGE_GRID_CELLS_PER_ROW => "imageGridCellsPerRow",
        keys::VIDEO_GRID_CELLS_PER_ROW => "videoGridCellsPerRow",
        keys::SHOW_HIDDEN_FILES => "showHiddenFiles",
        keys::FEED_AUTO_REFRESH => "feedAutoRefresh",
        keys::FEED_AUTO_REFRESH_INTERVAL => "feedAutoRefreshIntervalSec",
        keys::FEED_AUTO_REFRESH_ONLY_WIFI => "feedAutoRefreshOnlyWifi",
        keys::FEED_FONT_SCALE => "feedFontScale",
        keys::EDITOR_WRAP_CONTENT => "editorWrapContent",
        keys::EDITOR_FONT_SIZE => "editorFontSize",
        keys::EDITOR_STATUS_BAR => "editorStatusBar",
        keys::LAST_FILE_PATH => "lastFilePath",
        keys::SCAN_HISTORY => "scanHistory",
        keys::AUDIO_PLAYLIST => "audioPlaylist",
        keys::CHAT_INPUT_TEXT => "chatInputText",
        keys::NEARBY_DISCOVERABLE => "nearbyDiscoverable",
        keys::AI_IMAGE_SEARCH_ENABLED => "aiImageSearchEnabled",
        keys::NOTIFICATION_FILTER => "notificationFilter",
        keys::POMODORO_SETTINGS => "pomodoroSettings",
        keys::VIDEO_PLAYLIST => "videoPlaylist",
        keys::HOME_FEATURES => "homeFeatures",
        keys::QUICK_NOTE_DRAFT => "quickNoteDraft",
        keys::LAUNCHER_SHORTCUTS => "launcherShortcuts",
        keys::RECENT_SEARCHES => "recentSearches",
        keys::RECENT_SAVE_DIRS => "recentSaveDirs",
        keys::AUDIO_SORT_BY => "audioSortBy",
        keys::VIDEO_SORT_BY => "videoSortBy",
        keys::IMAGE_SORT_BY => "imageSortBy",
        keys::DOC_SORT_BY => "docSortBy",
        keys::FILE_SORT_BY => "fileSortBy",
        keys::PACKAGE_SORT_BY => "packageSortBy",
        _ => return Ok(None),
    };
    let patch = serde_json::from_value(serde_json::json!({field:value}))?;
    Ok(Some(self::patch(prefs, patch)?))
}
