pub fn desktop_capabilities() -> Vec<&'static str> {
    let mut capabilities = vec![
        "FILE_EXPORT",
        "FILE_REVEAL",
        "DOC_PREVIEW",
        "IMAGE_EDITOR",
        "NOTIFICATIONS",
        "NOTES",
        "FEEDS",
        "POMODORO",
        "APP_UPDATES",
    ];
    #[cfg(target_os = "macos")]
    capabilities.push("FILE_CLIPBOARD");
    capabilities.extend(plain_server::services::files::media_action_io::capabilities());
    capabilities
}

#[cfg(test)]
#[path = "../../tests/unit/host/device.rs"]
mod tests;
