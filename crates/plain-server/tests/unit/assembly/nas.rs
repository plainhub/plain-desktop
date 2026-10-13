use super::*;

#[test]
fn password_command_writes_shared_system_credentials() {
    let directory = tempfile::tempdir().unwrap();
    set_password(directory.path(), "example-password").unwrap();
    let prefs =
        plain_rs::prefs::Prefs::load(&plain_rs::prefs::default_path(directory.path())).unwrap();
    assert_eq!(
        prefs
            .get::<String>(plain_rs::prefs::system::keys::PASSWORD)
            .unwrap()
            .as_deref(),
        Some("example-password")
    );
}

#[test]
fn scan_sources_only_select_complete_usb_slot_paths() {
    for path in ["/mnt/usb1", "/mnt/usb12"] {
        assert!(is_usb_mount(path));
    }
    for path in [
        "/mnt/usb",
        "/mnt/usb0",
        "/mnt/usb1/photos",
        "/mnt/usb-1",
        "/mnt/usb1x",
        "/mnt/usb18446744073709551616",
    ] {
        assert!(!is_usb_mount(path));
    }
}
