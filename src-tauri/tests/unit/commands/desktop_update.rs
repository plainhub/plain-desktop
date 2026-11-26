use super::*;

#[test]
fn command_requires_the_native_about_window_and_exact_local_origin() {
    for url in ["tauri://localhost/about", "http://tauri.localhost/about"] {
        assert!(allowed_caller("about", &Url::parse(url).unwrap()));
        assert!(!allowed_caller("main", &Url::parse(url).unwrap()));
    }
    for url in [
        "https://d.plainapp.app/about",
        "tauri://localhost/other",
        "tauri://foreign/about",
        "http://tauri.localhost:4000/about",
    ] {
        assert!(!allowed_caller("about", &Url::parse(url).unwrap()));
    }
}

#[test]
fn release_version_is_strict_numeric_and_cannot_control_the_path() {
    assert!(stable_version("4.2.10").unwrap() > stable_version("4.2.9").unwrap());
    for value in ["4.0.1-beta", "4.00.1", "4.0", "4.0.0/../../x", "+4.0.1"] {
        assert!(stable_version(value).is_err());
    }
}

#[test]
fn auto_update_keeps_the_installed_bundle_format() {
    assert_eq!(
        supported_suffix("windows", "x86_64", BundleType::Msi),
        Some("windows.msi")
    );
    assert_eq!(
        supported_suffix("windows", "x86_64", BundleType::Nsis),
        Some("windows.exe")
    );
    assert_eq!(
        supported_suffix("macos", "aarch64", BundleType::App),
        Some("macos-aarch64.dmg")
    );
    assert_eq!(
        supported_suffix("macos", "x86_64", BundleType::App),
        Some("macos-x64.dmg")
    );
    assert_eq!(
        supported_suffix("linux", "x86_64", BundleType::AppImage),
        Some("linux.AppImage")
    );
    assert_eq!(supported_suffix("linux", "x86_64", BundleType::Deb), None);
    assert_eq!(supported_suffix("linux", "x86_64", BundleType::Rpm), None);
}

#[test]
fn altered_same_size_binary_fails_before_installing() {
    let directory = plain_libs::temporary::Builder::new().tempdir().unwrap();
    let path = directory.path().join("PlainApp-4.0.1-macos-aarch64.dmg");
    let expected = sha256_reader(&b"valid installer"[..]).unwrap();
    std::fs::write(&path, b"valid installer").unwrap();
    assert!(verify_package(&path, &expected).is_ok());
    std::fs::write(&path, b"other installer").unwrap();
    assert!(verify_package(&path, &expected).is_err());
}
