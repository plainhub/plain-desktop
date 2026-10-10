use super::UserSettingsPatch;
use anyhow::{Result, ensure};
use serde_json::Value;

fn strings(value: &Value) -> Result<Vec<String>> {
    Ok(serde_json::from_value(value.clone())?)
}
fn fields(value: &Value, names: &[&str], predicate: fn(&Value) -> bool) -> Result<()> {
    ensure!(value.is_object(), "Expected a settings object");
    for name in names {
        ensure!(
            value.get(*name).is_some_and(predicate),
            "Invalid settings field: {name}"
        );
    }
    Ok(())
}
fn integer(value: &Value) -> bool {
    value.as_i64().is_some()
}
fn positive(value: &Value) -> bool {
    value
        .as_i64()
        .is_some_and(|n| n > 0 && n <= i32::MAX as i64 / 60)
}

pub(super) fn validate(patch: &UserSettingsPatch) -> Result<()> {
    for port in [patch.http_port, patch.https_port].into_iter().flatten() {
        ensure!((0..=65535).contains(&port), "Invalid HTTP port");
    }
    for cells in [
        patch.image_grid_cells_per_row,
        patch.video_grid_cells_per_row,
    ]
    .into_iter()
    .flatten()
    {
        ensure!(cells > 0, "Invalid grid size");
    }
    for sort in [
        patch.audio_sort_by,
        patch.video_sort_by,
        patch.image_sort_by,
        patch.doc_sort_by,
        patch.file_sort_by,
        patch.package_sort_by,
    ]
    .into_iter()
    .flatten()
    {
        ensure!((0..=6).contains(&sort), "Invalid sort order");
    }
    if let Some(mode) = patch.audio_play_mode {
        ensure!((0..=2).contains(&mode), "Invalid audio play mode");
    }
    if let Some(speed) = patch.audio_playback_speed {
        ensure!(speed.is_finite() && speed > 0.0, "Invalid playback speed");
    }
    if let Some(mode) = patch.dark_theme {
        ensure!((0..=2).contains(&mode), "Invalid theme");
    }
    for value in [patch.feed_auto_refresh_interval, patch.editor_font_size]
        .into_iter()
        .flatten()
    {
        ensure!(value > 0, "Invalid settings value");
    }
    if let Some(value) = &patch.scan_history {
        strings(value)?;
    }
    for (value, limit) in [(&patch.recent_searches, 10), (&patch.recent_save_dirs, 5)] {
        if let Some(value) = value {
            ensure!(
                strings(value)?.len() <= limit,
                "Recent list exceeds its limit"
            );
        }
    }
    if let Some(value) = &patch.last_file_path {
        fields(
            value,
            &["rootPath", "fullPath", "selectedPath"],
            Value::is_string,
        )?;
    }
    if let Some(value) = &patch.screen_mirror_quality {
        fields(value, &["resolution"], positive)?;
        ensure!(
            matches!(value["mode"].as_str(), Some("HD" | "SMOOTH")),
            "Invalid screen mirror mode"
        );
    }
    if let Some(value) = &patch.notification_filter {
        ensure!(
            matches!(value["mode"].as_str(), Some("allowlist" | "blacklist")),
            "Invalid notification mode"
        );
        strings(&value["apps"])?;
    }
    if let Some(value) = &patch.pomodoro_settings {
        fields(
            value,
            &[
                "workDurationMin",
                "shortBreakDurationMin",
                "longBreakDurationMin",
                "pomodorosBeforeLongBreak",
            ],
            positive,
        )?;
        fields(
            value,
            &["showNotification", "playSoundOnComplete"],
            Value::is_boolean,
        )?;
        fields(value, &["soundPath", "originalSoundName"], Value::is_string)?;
    }
    for (value, video) in [
        (&patch.audio_playlist, false),
        (&patch.video_playlist, true),
    ] {
        if let Some(value) = value {
            for item in value
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("Expected a playlist"))?
            {
                fields(item, &["title", "path"], Value::is_string)?;
                fields(item, &["durationMs"], integer)?;
                if video {
                    fields(
                        item,
                        &["id", "bucketId", "createdAt", "updatedAt"],
                        Value::is_string,
                    )?;
                    fields(item, &["size", "width", "height", "rotation"], integer)?;
                    ensure!(
                        item.get("takenAt")
                            .is_some_and(|v| v.is_null() || v.is_string()),
                        "Invalid takenAt"
                    );
                } else {
                    fields(item, &["artist"], Value::is_string)?;
                }
            }
        }
    }
    Ok(())
}
