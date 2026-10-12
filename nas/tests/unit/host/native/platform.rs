use super::*;

#[test]
fn app_update_reports_the_package_version_without_inventing_an_update() {
    let facts = app_update().expect("the package version is always available");
    assert_eq!(facts.current_version, crate::version::VERSION);
    assert!(facts.latest_version.is_none());
    assert!(!facts.has_update);
    assert!(facts.url.is_none());
}

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

#[test]
fn disks_either_reports_real_devices_or_an_error() {
    match disks() {
        Err(_) => {}
        Ok(facts) => {
            assert!(!facts.disks.is_empty());
            for disk in &facts.disks {
                assert!(!disk.id.is_empty(), "a disk without an id is not a fact");
                assert!(disk.path.starts_with("/dev/"), "path {}", disk.path);
                assert!(!disk.name.is_empty());
            }
        }
    }
}
