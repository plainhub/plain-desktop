use plain_server::assembly::server::ContentServer;
use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[tauri::command]
pub fn native_view_scope(
    window: tauri::WebviewWindow,
    views: tauri::State<'_, super::views::NativeViews>,
) -> String {
    views.scope(window.label()).id().to_owned()
}

fn request_deadline(body: &serde_json::Value, now: u64) -> Result<Duration, String> {
    let deadline = body
        .pointer("/extensions/nativeDeadlineMs")
        .and_then(serde_json::Value::as_u64)
        .ok_or("Native request deadline required")?;
    let remaining = deadline
        .checked_sub(now)
        .filter(|remaining| *remaining > 0)
        .ok_or("connection_timeout")?;
    Ok(Duration::from_millis(remaining))
}

#[tauri::command]
pub async fn native_graphql(
    window: tauri::WebviewWindow,
    views: tauri::State<'_, super::views::NativeViews>,
    core: tauri::State<'_, Arc<ContentServer>>,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let id = body
        .pointer("/extensions/nativeScope")
        .and_then(serde_json::Value::as_str)
        .ok_or("Native request scope required")?;
    let scope = views.existing_scope(window.label(), id)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis();
    let remaining = request_deadline(
        &body,
        u64::try_from(now).map_err(|error| error.to_string())?,
    )?;
    let executor = core.native_graphql()?;
    let request = serde_json::from_value(body).map_err(|error| error.to_string())?;
    let response = tokio::time::timeout(remaining, executor.execute_scoped(request, scope))
        .await
        .map_err(|_| "connection_timeout".to_owned())?;
    serde_json::to_value(response).map_err(|error| error.to_string())
}

#[cfg(test)]
#[path = "../../../tests/unit/commands/native/graphql.rs"]
mod tests;
