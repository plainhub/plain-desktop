use super::*;

#[test]
fn every_declared_capability_is_unique_and_understood_by_the_main_schema() {
    let capabilities = desktop_capabilities();
    let distinct = capabilities
        .iter()
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(capabilities.len(), distinct.len());
    for name in capabilities {
        assert!(Capability::parse(name.as_str()) == Some(name), "{name}");
    }
}

#[test]
fn desktop_does_not_advertise_base_file_or_media_capabilities() {
    let capabilities = desktop_capabilities();
    for name in [
        "FILE_BROWSER",
        "IMAGE_LIBRARY",
        "AUDIO_LIBRARY",
        "VIDEO_LIBRARY",
        "DOCUMENT_LIBRARY",
    ] {
        assert!(!capabilities.iter().any(|capability| capability.as_str() == name));
    }
    assert!(!capabilities.iter().any(|capability| capability.as_str() == "LAUNCHER_SHORTCUTS"));
}

#[test]
fn text_clipboard_is_available_through_the_native_platform_adapter() {
    assert!(desktop_capabilities().contains(&Capability::Clipboard));
}

#[test]
fn native_media_catalog_scanning_is_available_to_the_main_schema() {
    assert!(desktop_capabilities().contains(&Capability::MediaScan));
}

#[test]
fn local_system_notification_management_is_not_advertised() {
    assert!(!desktop_capabilities().contains(&Capability::Notifications));
}

#[test]
fn local_image_trash_is_available_to_the_main_schema() {
    assert!(desktop_capabilities().contains(&Capability::MediaTrash));
}
