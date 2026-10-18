use std::sync::Arc;

use plain_server::assembly::server::ContentServer;
use tauri::ipc::{Channel, InvokeResponseBody};

#[tauri::command]
pub async fn native_events(
    window: tauri::WebviewWindow,
    views: tauri::State<'_, super::views::NativeViews>,
    core: tauri::State<'_, Arc<ContentServer>>,
    on_event: Channel<InvokeResponseBody>,
) -> Result<(), String> {
    let executor = core.native_graphql()?;
    let mut events = executor.subscribe_events(views.scope(window.label()))?;
    while let Some(frame) = events.next_frame().await? {
        on_event
            .send(InvokeResponseBody::Raw(frame))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
