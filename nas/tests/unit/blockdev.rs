//! Unit tests for `src/blockdev.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

#[test]
fn base_block_device_names() {
    // SATA/SCSI/USB partitions strip the trailing number.
    assert_eq!(base_block_device_name("sda3"), "sda");
    assert_eq!(base_block_device_name("sdb"), "sdb");
    // NVMe: namespace stays, partition suffix strips.
    assert_eq!(base_block_device_name("nvme0n1p2"), "nvme0n1");
    assert_eq!(base_block_device_name("nvme0n1"), "nvme0n1");
    assert_eq!(base_block_device_name("nvme1n2"), "nvme1n2");
    // MMC: p<part> strips, whole disk stays.
    assert_eq!(base_block_device_name("mmcblk0p1"), "mmcblk0");
    assert_eq!(base_block_device_name("mmcblk1"), "mmcblk1");
    // MD RAID mirrors MMC handling.
    assert_eq!(base_block_device_name("md0p2"), "md0");
    assert_eq!(base_block_device_name("md127"), "md127");
    // Device mapper keeps the dash (same as Go).
    assert_eq!(base_block_device_name("dm-3"), "dm-");
}

#[test]
fn all_digits_cases() {
    assert!(all_digits("123"));
    assert!(all_digits("0"));
    assert!(!all_digits(""));
    assert!(!all_digits("12a"));
    assert!(!all_digits("a12"));
}

#[test]
fn user_visible_disk_names() {
    assert!(is_user_visible_disk_name("sda"));
    assert!(is_user_visible_disk_name("nvme0n1"));
    assert!(!is_user_visible_disk_name("zram0"));
    assert!(!is_user_visible_disk_name("loop0"));
    assert!(!is_user_visible_disk_name("ram0"));
    assert!(!is_user_visible_disk_name(""));
    assert!(!is_user_visible_disk_name("   "));
}

#[test]
fn parse_boolish_variants() {
    use serde_json::json;
    assert_eq!(parse_boolish(&Some(json!(true))), Some(true));
    assert_eq!(parse_boolish(&Some(json!(false))), Some(false));
    assert_eq!(parse_boolish(&Some(json!(1))), Some(true));
    assert_eq!(parse_boolish(&Some(json!(0))), Some(false));
    assert_eq!(parse_boolish(&Some(json!("1"))), Some(true));
    assert_eq!(parse_boolish(&Some(json!("yes"))), Some(true));
    assert_eq!(parse_boolish(&Some(json!("No"))), Some(false));
    assert_eq!(parse_boolish(&Some(json!("0"))), Some(false));
    // Absent / unrecognized → None (caller falls back to sysfs).
    assert_eq!(parse_boolish(&None), None);
    assert_eq!(parse_boolish(&Some(json!("maybe"))), None);
    assert_eq!(parse_boolish(&Some(json!(""))), None);
    assert_eq!(parse_boolish(&Some(json!(null))), None);
}

#[test]
fn disk_id_falls_back_to_kernel_name() {
    // No /dev/disk/by-id link can target a nonexistent device, so the
    // fallback path is deterministic across machines.
    assert_eq!(
        disk_id_from_name("definitely-not-real-xyz"),
        "disk:definitely-not-real-xyz"
    );
    assert_eq!(disk_id_from_name(""), "");
    assert_eq!(disk_id_from_name("  "), "");
}

#[test]
fn best_by_id_for_missing_device_is_none() {
    assert_eq!(
        best_by_id_name_for_dev_path("/dev/definitely-not-real-xyz"),
        None
    );
    assert_eq!(best_by_id_name_for_dev_path(""), None);
}

#[test]
fn flatten_devices_walks_children_depth_first() {
    let tree = vec![LsblkDevice {
        name: "sda".into(),
        children: vec![
            LsblkDevice {
                name: "sda1".into(),
                children: vec![],
                ..Default::default()
            },
            LsblkDevice {
                name: "sda2".into(),
                children: vec![],
                ..Default::default()
            },
        ],
        ..Default::default()
    }];
    let names: Vec<&str> = flatten_devices(&tree)
        .iter()
        .map(|d| d.name.as_str())
        .collect();
    assert_eq!(names, vec!["sda", "sda1", "sda2"]);
}
