use crate::host::bridge::Host;
use serde_json::json;
pub(crate) async fn receipt(host: &Host, method: &str, path: &str) -> Result<(), String> {
    let value = host.call(method, json!({"paths":[path]})).await?;
    if value.as_bool() != Some(true) {
        return Err("invalid file host receipt".into());
    }
    Ok(())
}

pub(crate) async fn authorize_read(host: &Host, path: &str) -> Result<(), String> {
    let value = host
        .call(
            "fileTaskAuthorize",
            json!({"operation":"read","paths":[path]}),
        )
        .await?;
    if value.as_bool() != Some(true) {
        return Err("invalid file host receipt".into());
    }
    Ok(())
}

pub(crate) async fn resolve_native(
    state: &crate::assembly::server::ServerState,
    reference: &str,
) -> Result<std::path::PathBuf, String> {
    state.native_client_id()?;
    let path = crate::host::filesystem::resolve(state, reference)
        .await
        .map_err(|error| error.to_string())?;
    let path = std::path::PathBuf::from(path);
    if !path.is_absolute() {
        return Err("absolute native file path required".into());
    }
    authorize_read(
        &state.host,
        path.to_str().ok_or("invalid file path encoding")?,
    )
    .await?;
    state.native_client_id()?;
    Ok(path)
}
