use super::*;

const MOUNTS: &str = "\
proc /proc proc rw,nosuid,nodev,noexec,relatime 0 0
/dev/nvme0n1p2 / ext4 rw,relatime 0 0
/dev/nvme0n1p1 /boot/efi vfat rw,relatime 0 0
tmpfs /run tmpfs rw,nosuid,nodev 0 0
/dev/sdb1 /media/usb\\040archive vfat rw,relatime 0 0
/dev/mapper/vg-data /srv/data xfs rw,relatime 0 0
";

#[test]
fn parses_only_block_backed_mounts() {
    let entries = parse_mounts(MOUNTS);
    let devices: Vec<&str> = entries.iter().map(|entry| entry.device.as_str()).collect();
    assert_eq!(
        devices,
        [
            "/dev/nvme0n1p2",
            "/dev/nvme0n1p1",
            "/dev/sdb1",
            "/dev/mapper/vg-data"
        ]
    );
}

#[test]
fn decodes_escaped_mount_points() {
    let entries = parse_mounts(MOUNTS);
    let usb = entries
        .iter()
        .find(|entry| entry.device == "/dev/sdb1")
        .expect("the usb volume is a block-backed mount");
    assert_eq!(usb.mount_point, "/media/usb archive");
}

#[test]
fn nested_mount_points_are_recognised_as_children() {
    assert!(!is_nested("/", "/"));
    assert!(is_nested("/boot/efi", "/"));
    assert!(is_nested("/media/usb/inner", "/media/usb"));
    assert!(!is_nested("/media/usb2", "/media/usb"));
}

#[test]
fn the_root_volume_is_named_after_its_device() {
    let root = MountEntry {
        device: "/dev/nvme0n1p2".to_string(),
        mount_point: "/".to_string(),
    };
    assert_eq!(disk_name(&root), "nvme0n1p2");

    let usb = MountEntry {
        device: "/dev/sdb1".to_string(),
        mount_point: "/media/usb archive".to_string(),
    };
    assert_eq!(disk_name(&usb), "usb archive");
}

#[test]
fn maps_the_existing_update_check_into_shared_facts() {
    let check = UpdateCheck {
        current_version: "0.1.12".to_string(),
        latest_version: "0.2.0".to_string(),
        has_update: true,
        release_name: "Desktop 0.2.0".to_string(),
        release_url: "https://example.invalid/releases/0.2.0".to_string(),
        published_at: "2026-01-01T00:00:00Z".to_string(),
    };
    let facts = update_facts("0.1.12".to_string(), check);
    assert_eq!(facts.current_version, "0.1.12");
    assert_eq!(facts.latest_version.as_deref(), Some("0.2.0"));
    assert!(facts.has_update);
    assert_eq!(
        facts.url.as_deref(),
        Some("https://example.invalid/releases/0.2.0")
    );
}

#[test]
fn reported_disks_are_backed_by_a_real_device() {
    let Ok(facts) = disks() else {
        return;
    };
    for disk in &facts.disks {
        assert!(
            disk.id.starts_with("/dev/"),
            "id {} is not a device",
            disk.id
        );
        assert!(
            disk.path.starts_with('/'),
            "path {} is not absolute",
            disk.path
        );
        assert!(!disk.name.is_empty());
        assert!(!facts.disks.iter().any(|other| other.path == disk.path));
    }
}

#[test]
fn a_readable_host_without_disks_is_an_empty_list() {
    let facts = disks_facts(Ok(Vec::new())).expect("an empty read is not a failure");

    assert!(facts.disks.is_empty());
}

#[test]
fn an_unreadable_host_reports_the_operating_system_error() {
    let error = disks_facts(Err("cannot read /proc/self/mounts".to_string()))
        .expect_err("a failed read must not look like an empty host");

    assert_eq!(error, "cannot read /proc/self/mounts");
}
