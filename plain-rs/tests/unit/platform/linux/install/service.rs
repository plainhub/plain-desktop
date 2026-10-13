use super::*;

#[test]
fn install_updates_existing_unit_to_the_actual_binary() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("system/plain-nas.service");
    write_service(&path, "/usr/local/bin/plain-nas").unwrap();
    write_service(&path, BINARY_PATH).unwrap();
    let unit = std::fs::read_to_string(&path).unwrap();
    assert!(unit.contains("ExecStart=/usr/local/bin/plainapp run\n"));
    assert!(!unit.contains("ExecStart=/usr/local/bin/plain-nas"));
    assert!(unit.contains("Restart=on-failure"));
}

#[test]
fn service_write_failure_is_returned() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("file");
    std::fs::write(&file, "bytes").unwrap();
    assert!(write_service(&file.join("unit"), BINARY_PATH).is_err());
    assert_eq!(std::fs::read_to_string(file).unwrap(), "bytes");
}
