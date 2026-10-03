use super::server::ServerState;
use serde_json::json;
pub(super) async fn receipt(state: &ServerState, method: &str, path: &str) -> Result<(), String> {
    let value = state.host.call(method, json!({"paths":[path]})).await?;
    if value.as_bool() != Some(true) {
        return Err("invalid file host receipt".into());
    }
    Ok(())
}
