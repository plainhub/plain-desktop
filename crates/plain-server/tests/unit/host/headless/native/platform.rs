use super::*;

#[cfg(target_os = "linux")]
#[test]
fn a_readable_host_without_disks_is_an_empty_list() {
    let facts = disks_facts(Ok(Vec::new())).expect("an empty read is not a failure");

    assert!(facts.disks.is_empty());
}

#[cfg(target_os = "linux")]
#[test]
fn a_failed_read_is_reported_with_its_own_error() {
    let error = disks_facts(Err("lsblk spawn: no such file".to_string()))
        .err()
        .expect("a failed read must not look like an empty host");

    assert_eq!(error, "lsblk spawn: no such file");
}

#[test]
fn a_newer_release_is_carried_into_the_shared_facts() {
    let facts = update_facts(AppUpdate {
        current_version: "0.1.12".to_string(),
        latest_version: Some("0.2.0".to_string()),
        has_update: true,
        url: Some("https://example.invalid/releases/0.2.0".to_string()),
    });
    assert_eq!(facts.current_version, "0.1.12");
    assert_eq!(facts.latest_version.as_deref(), Some("0.2.0"));
    assert!(facts.has_update);
    assert_eq!(
        facts.url.as_deref(),
        Some("https://example.invalid/releases/0.2.0")
    );
}

#[test]
fn an_unreachable_release_lookup_invents_no_update() {
    let facts = update_facts(AppUpdate {
        current_version: "0.1.12".to_string(),
        latest_version: None,
        has_update: false,
        url: None,
    });
    assert_eq!(facts.current_version, "0.1.12");
    assert!(facts.latest_version.is_none());
    assert!(!facts.has_update);
    assert!(facts.url.is_none());
}

#[cfg(target_os = "linux")]
#[test]
fn maps_the_platform_disk_onto_the_shared_fact_shape() {
    let disk = Disk {
        id: "disk:sda".to_string(),
        name: "sda".to_string(),
        path: "/dev/sda".to_string(),
        size_bytes: 500_107_862_016,
        removable: true,
        model: Some("Samsung SSD 870 EVO".to_string()),
    };
    let facts = to_facts(disk);
    assert_eq!(facts.id, "disk:sda");
    assert_eq!(facts.name, "sda");
    assert_eq!(facts.path, "/dev/sda");
    assert_eq!(facts.size_bytes, 500_107_862_016);
    assert!(facts.removable);
    assert_eq!(facts.model.as_deref(), Some("Samsung SSD 870 EVO"));
}

#[cfg(target_os = "linux")]
#[test]
fn a_disk_without_a_known_model_stays_unknown() {
    let facts = to_facts(Disk {
        id: "disk:sdb".to_string(),
        name: "sdb".to_string(),
        path: "/dev/sdb".to_string(),
        size_bytes: 0,
        removable: false,
        model: None,
    });
    assert!(facts.model.is_none());
    assert_eq!(facts.size_bytes, 0);
}

#[cfg(target_os = "linux")]
#[test]
fn disks_either_reports_real_devices_or_an_error() {
    match disks() {
        Err(_) => {}
        Ok(facts) => {
            for disk in &facts.disks {
                assert!(!disk.id.is_empty(), "a disk without an id is not a fact");
                assert!(disk.path.starts_with("/dev/"), "path {}", disk.path);
                assert!(!disk.name.is_empty());
            }
        }
    }
}
