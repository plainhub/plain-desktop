use std::path::Path;

use crate::host::platform::AppFacts;

const STORAGE: &str = "WRITE_EXTERNAL_STORAGE";
const DEVICE_TYPE: &str = "COMPUTER";
const BUILD_CHANNEL: &str = "GITHUB";

pub fn app_facts(data_directory: &Path, capabilities: Vec<String>) -> Result<AppFacts, String> {
    if data_directory.is_relative() {
        return Err(format!(
            "app data directory must be absolute: {}",
            data_directory.display()
        ));
    }
    Ok(AppFacts {
        app_dir: data_directory.join("files").to_string_lossy().into_owned(),
        device_type: DEVICE_TYPE.to_string(),
        capabilities,
        build_channel: BUILD_CHANNEL.to_string(),
        permissions: vec![STORAGE.to_string()],
        downloads_dir: String::new(),
        debug: cfg!(debug_assertions),
    })
}

#[cfg(test)]
#[path = "../../../../tests/unit/host/headless/native/app.rs"]
mod tests;
