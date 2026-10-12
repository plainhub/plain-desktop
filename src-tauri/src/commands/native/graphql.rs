use std::sync::Arc;

use plain_server::assembly::server::ContentServer;

// `async_graphql` is not a shell dependency, so the executor's own signature infers both types.
#[tauri::command]
pub async fn native_graphql(
    core: tauri::State<'_, Arc<ContentServer>>,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let executor = core.native_graphql()?;
    let request = serde_json::from_value(body).map_err(|error| error.to_string())?;
    let response = executor.execute(request).await;
    serde_json::to_value(response).map_err(|error| error.to_string())
}
