use plain_rs::features::capability::Capability;

const DESKTOP: &[Capability] = &[
    Capability::MediaScan,
    Capability::Clipboard,
    Capability::AppUpdates,
];

pub fn desktop_capabilities() -> Vec<Capability> {
    let mut capabilities = DESKTOP.to_vec();
    #[cfg(target_os = "macos")]
    capabilities.push(Capability::Ble);
    capabilities.extend(plain_rs::features::services::files::media_action_io::capabilities());
    capabilities
}

#[cfg(test)]
#[path = "../../tests/unit/host/device.rs"]
mod tests;
