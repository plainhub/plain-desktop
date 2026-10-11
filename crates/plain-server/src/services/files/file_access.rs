use crate::host::bridge::Host;
use serde_json::json;
pub(crate) async fn receipt(host: &Host, method: &str, path: &str) -> Result<(), String> {
    let value = host.call(method, json!({"paths":[path]})).await?;
    if value.as_bool() != Some(true) {
        return Err("invalid file host receipt".into());
    }
    Ok(())
}
