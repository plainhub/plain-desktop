use super::*;

fn facts(capabilities: Vec<String>) -> AppFacts {
    app_facts(Path::new("/data/plain"), capabilities).expect("absolute data directory")
}

#[test]
fn app_dir_is_the_files_directory_below_the_data_directory() {
    assert_eq!(facts(Vec::new()).app_dir, "/data/plain/files");
}

#[test]
fn a_relative_data_directory_is_rejected() {
    let Err(error) = app_facts(Path::new("data/plain"), Vec::new()) else {
        panic!("a relative data directory must be rejected");
    };
    assert!(error.contains("absolute"), "{error}");
}

#[test]
fn capabilities_are_reported_exactly_as_given() {
    let facts = facts(vec!["MEDIA_TRASH".to_string(), "MEDIA_SCAN".to_string()]);
    assert_eq!(facts.capabilities, ["MEDIA_TRASH", "MEDIA_SCAN"]);
}

#[test]
fn the_desktop_app_facts_match_the_public_device_contract() {
    let facts = facts(Vec::new());
    assert_eq!(facts.device_type, "COMPUTER");
    assert_eq!(facts.build_channel, "GITHUB");
    assert_eq!(facts.permissions, ["WRITE_EXTERNAL_STORAGE"]);
    assert_eq!(facts.downloads_dir, "");
    assert_eq!(facts.debug, cfg!(debug_assertions));
}
