use super::*;

fn fixture() -> plain_libs::temporary::TempDir {
    plain_libs::temporary::tempdir().unwrap()
}

#[test]
fn atomic_bundle_replacement_preserves_old_bundle_in_staging() {
    let stage = fixture();
    let installed = stage.path().join("Installed.app");
    let replacement = stage.path().join("New.app");
    std::fs::create_dir(&installed).unwrap();
    std::fs::write(installed.join("version"), b"old").unwrap();
    std::fs::create_dir(&replacement).unwrap();
    std::fs::write(replacement.join("version"), b"new").unwrap();

    swap_bundles(&installed, &replacement).unwrap();
    assert_eq!(std::fs::read(installed.join("version")).unwrap(), b"new");
    assert_eq!(std::fs::read(replacement.join("version")).unwrap(), b"old");
}

#[test]
fn failed_atomic_replacement_does_not_move_installed_bundle() {
    let stage = fixture();
    let installed = stage.path().join("Installed.app");
    std::fs::create_dir(&installed).unwrap();
    std::fs::write(installed.join("version"), b"old").unwrap();

    assert!(swap_bundles(&installed, &stage.path().join("Missing.app")).is_err());
    assert_eq!(std::fs::read(installed.join("version")).unwrap(), b"old");
}

#[test]
fn app_store_bundle_is_not_a_direct_download_install_target() {
    let stage = fixture();
    let bundle = std::fs::canonicalize(stage.path())
        .unwrap()
        .join("PlainApp.app");
    let executable = bundle.join(EXECUTABLE);
    std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
    std::fs::write(&executable, b"test binary").unwrap();
    require_bundle(&bundle).unwrap();

    std::fs::create_dir_all(bundle.join("Contents/_MASReceipt")).unwrap();
    assert!(require_bundle(&bundle)
        .unwrap_err()
        .contains("Mac App Store"));
}

#[test]
fn valid_adhoc_signature_is_not_authorized_for_update() {
    let stage = fixture();
    let executable = stage.path().join("candidate");
    std::fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    command(
        Command::new("/usr/bin/codesign")
            .args([
                "--force",
                "--sign",
                "-",
                "--identifier",
                "com.ismartcoding.plain.desktop",
            ])
            .arg(&executable),
    )
    .unwrap();
    verify_code(
        &executable,
        r#"identifier "com.ismartcoding.plain.desktop""#,
    )
    .unwrap();

    assert!(verify_code(&executable, DEVELOPER_ID).is_err());
}
