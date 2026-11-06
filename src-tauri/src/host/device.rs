pub fn desktop_capabilities() -> Vec<&'static str> {
    let mut capabilities = vec!["MEDIA_SCAN", "CLIPBOARD", "APP_UPDATES"];
    capabilities.extend(plain_rs::features::services::files::media_action_io::capabilities());
    capabilities
}

#[cfg(test)]
#[path = "../../tests/unit/host/device.rs"]
mod tests;
