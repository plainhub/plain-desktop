pub fn desktop_capabilities() -> Vec<&'static str> {
    let mut capabilities = vec![
        "PEER_DISCOVERY",
        "FILE_BROWSER",
        "IMAGE_LIBRARY",
        "AUDIO_LIBRARY",
        "VIDEO_LIBRARY",
        "DOCUMENT_LIBRARY",
        "FILE_EXPORT",
        "FILE_REVEAL",
        "DOC_PREVIEW",
        "IMAGE_EDITOR",
        "NOTIFICATIONS",
        "NOTES",
        "FEEDS",
        "BACKGROUND_TIMER",
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
