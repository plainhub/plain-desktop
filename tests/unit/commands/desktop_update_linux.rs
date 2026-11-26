use super::*;

fn image(machine: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = b"\x7fELF\x02\x01\x01\0AI\x02\0\0\0\0\0\x02\0\0\0".to_vec();
    bytes[18] = machine;
    bytes.extend(payload);
    bytes
}

fn fixture() -> plain_libs::temporary::TempDir {
    plain_libs::temporary::tempdir().unwrap()
}

#[test]
fn replacement_keeps_executable_mode_and_updates_only_the_appimage() {
    let stage = fixture();
    let installed = stage.path().join("PlainApp-old.AppImage");
    let new_file = stage.path().join("download");
    let settings = stage.path().join("settings.json");
    fs::write(&installed, image(62, b"old")).unwrap();
    fs::set_permissions(&installed, Permissions::from_mode(0o755)).unwrap();
    fs::write(&new_file, image(62, b"new")).unwrap();
    fs::write(&settings, b"existing settings").unwrap();

    if fs::metadata(&installed).unwrap().uid() == 0 {
        assert!(replace(&installed, &new_file, stage.path()).is_err());
        assert_eq!(fs::read(installed).unwrap(), image(62, b"old"));
        return;
    }
    replace(&installed, &new_file, stage.path()).unwrap();
    assert_eq!(fs::read(&installed).unwrap(), image(62, b"new"));
    assert_eq!(fs::metadata(installed).unwrap().mode() & 0o777, 0o755);
    assert_eq!(fs::read(settings).unwrap(), b"existing settings");
}

#[test]
fn invalid_or_wrong_architecture_download_does_not_replace_appimage() {
    let stage = fixture();
    let installed = stage.path().join("PlainApp-old.AppImage");
    let new_file = stage.path().join("download");
    let old = image(62, b"old");
    fs::write(&installed, &old).unwrap();

    for bytes in [image(183, b"wrong architecture"), b"not AppImage".to_vec()] {
        fs::write(&new_file, bytes).unwrap();
        assert!(replace(&installed, &new_file, stage.path()).is_err());
        assert_eq!(fs::read(&installed).unwrap(), old);
    }
}

#[test]
fn an_open_instance_still_holds_the_old_inode_after_update() {
    let stage = fixture();
    let installed = stage.path().join("PlainApp-old.AppImage");
    let new_file = stage.path().join("download");
    let old = image(62, b"old");
    fs::write(&installed, &old).unwrap();
    fs::write(&new_file, image(62, b"new")).unwrap();
    let mut running = File::open(&installed).unwrap();
    if running.metadata().unwrap().uid() == 0 {
        return;
    }
    replace(&installed, &new_file, stage.path()).unwrap();

    let mut running_bytes = Vec::new();
    running.read_to_end(&mut running_bytes).unwrap();
    assert_eq!(running_bytes, old);
    assert_eq!(fs::read(installed).unwrap(), image(62, b"new"));
}
