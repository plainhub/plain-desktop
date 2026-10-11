use super::*;
use serde::{Deserialize, Serialize};

/// A user-settings command that is more than a field write: it enforces
/// limits, de-duplication and cross-field rules, so it stays an action rather
/// than a patch. Tagged so a non-GraphQL caller (the App's native bridge) can
/// dispatch it without a hand-written match.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", content = "value", rename_all = "camelCase")]
pub enum Action {
    RecordSearch(String),
    RemoveSearch(String),
    ClearSearches,
    RecordSaveDirectory(String),
    ToggleNotificationApp(String),
    SetNotificationMode(String),
    DeleteVideos(Vec<String>),
    ClearNotificationApps,
}
impl Action {
    pub fn apply(self, prefs: &Prefs) -> anyhow::Result<bool> {
        let record = matches!(&self, Self::RecordSearch(_));
        let toggle = matches!(&self, Self::ToggleNotificationApp(_));
        if let Self::SetNotificationMode(ref mode) = self {
            anyhow::ensure!(
                ["allowlist", "blacklist"].contains(&mode.as_str()),
                "Invalid notification mode"
            );
        }
        let mut changed = false;
        prefs.update_user(|entries| {
            let defaults = UserSettings::default();
            let (key, value) = match self {
                Self::RecordSearch(term) | Self::RemoveSearch(term) => {
                    let mut items: Vec<String> = serde_json::from_value(
                        entries
                            .get(keys::RECENT_SEARCHES)
                            .cloned()
                            .unwrap_or(defaults.recent_searches),
                    )
                    .map_err(PrefsError::Decode)?;
                    items.retain(|item| !same_term(item, &term));
                    if record {
                        items.insert(0, term);
                        items.truncate(10);
                    }
                    (
                        keys::RECENT_SEARCHES,
                        serde_json::to_value(items).map_err(PrefsError::Encode)?,
                    )
                }
                Self::ClearSearches => (keys::RECENT_SEARCHES, serde_json::json!([])),
                Self::RecordSaveDirectory(path) => {
                    let mut items: Vec<String> = serde_json::from_value(
                        entries
                            .get(keys::RECENT_SAVE_DIRS)
                            .cloned()
                            .unwrap_or(defaults.recent_save_dirs),
                    )
                    .map_err(PrefsError::Decode)?;
                    items.retain(|item| item != &path);
                    items.insert(0, path);
                    items.truncate(5);
                    (
                        keys::RECENT_SAVE_DIRS,
                        serde_json::to_value(items).map_err(PrefsError::Encode)?,
                    )
                }
                Self::ToggleNotificationApp(app) | Self::SetNotificationMode(app) => {
                    let mut filter = entries
                        .get(keys::NOTIFICATION_FILTER)
                        .cloned()
                        .unwrap_or(defaults.notification_filter);
                    if toggle {
                        let mut apps: Vec<Value> = serde_json::from_value(filter["apps"].clone())
                            .map_err(PrefsError::Decode)?;
                        let value = Value::String(app);
                        if apps.contains(&value) {
                            apps.retain(|a| a != &value);
                        } else {
                            apps.push(value);
                        }
                        filter["apps"] = Value::Array(apps);
                    } else {
                        filter["mode"] = Value::String(app);
                    }
                    (keys::NOTIFICATION_FILTER, filter)
                }
                Self::ClearNotificationApps => {
                    let mut filter = entries
                        .get(keys::NOTIFICATION_FILTER)
                        .cloned()
                        .unwrap_or(defaults.notification_filter);
                    filter["apps"] = serde_json::json!([]);
                    (keys::NOTIFICATION_FILTER, filter)
                }
                Self::DeleteVideos(paths) => {
                    let mut videos: Vec<Value> = serde_json::from_value(
                        entries
                            .get(keys::VIDEO_PLAYLIST)
                            .cloned()
                            .unwrap_or(defaults.video_playlist),
                    )
                    .map_err(PrefsError::Decode)?;
                    videos.retain(|v| !paths.iter().any(|p| v["path"].as_str() == Some(p)));
                    (keys::VIDEO_PLAYLIST, Value::Array(videos))
                }
            };
            if entries.get(key) != Some(&value) {
                entries.insert(key.into(), value);
                changed = true;
            }
            Ok(())
        })?;
        Ok(changed)
    }
}

// Kotlin compares simple character case mappings, without expanding ligatures.
fn same_term(left: &str, right: &str) -> bool {
    fn upper(c: char) -> char {
        let mut mapping = c.to_uppercase();
        let first = mapping.next().unwrap();
        if mapping.next().is_none() { first } else { c }
    }
    left.chars().count() == right.chars().count()
        && left.chars().zip(right.chars()).all(|(a, b)| {
            a == b || upper(a) == upper(b) || a.to_lowercase().next() == b.to_lowercase().next()
        })
}
