//! Unit tests for `src/storage_disks.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

fn dev(name: &str, kind: &str, rm: Option<serde_json::Value>) -> LsblkDevice {
    LsblkDevice {
        name: name.into(),
        path: format!("/dev/{name}"),
        kind: kind.into(),
        size: Some(1_000_000_000),
        rm,
        model: Some("Test Model".into()),
        ..Default::default()
    }
}

#[test]
fn only_top_level_disks_survive() {
    let devs = vec![
        dev("sda", "disk", Some(serde_json::json!(false))),
        // A partition listed at top level must be dropped.
        dev("sda1", "part", None),
        // Virtual devices hidden from the NAS UI.
        dev("zram0", "disk", None),
        dev("loop0", "disk", None),
        dev("ram0", "disk", None),
        // ROM drive is not TYPE=disk.
        dev("sr0", "rom", None),
    ];
    let disks = disks_from_lsblk(&devs);
    assert_eq!(disks.len(), 1);
    assert_eq!(disks[0].name, "sda");
    assert_eq!(disks[0].path, "/dev/sda");
    assert_eq!(disks[0].size_bytes, 1_000_000_000);
    assert!(!disks[0].removable);
}

#[test]
fn removable_from_rm_bool() {
    let disks = disks_from_lsblk(&[dev("sdb", "disk", Some(serde_json::json!(true)))]);
    assert!(disks[0].removable);
}

#[test]
fn removable_boolish_string() {
    let disks = disks_from_lsblk(&[dev("sdc", "disk", Some(serde_json::json!("1")))]);
    assert!(disks[0].removable);
}

#[test]
fn path_defaults_to_dev_slash_name() {
    let d = LsblkDevice {
        name: "sdd".into(),
        path: "".into(),
        kind: "disk".into(),
        rm: None,
        ..Default::default()
    };
    let disks = disks_from_lsblk(&[d]);
    assert_eq!(disks[0].path, "/dev/sdd");
}

#[test]
fn id_uses_disk_prefix_when_no_by_id() {
    // No /dev/disk/by-id entry can resolve to a nonexistent device.
    let disks = disks_from_lsblk(&[dev("definitely-not-real-abc", "disk", None)]);
    assert_eq!(disks[0].id, "disk:definitely-not-real-abc");
}

#[test]
fn lsblk_model_used_when_sysfs_missing() {
    // read_sys_block_model misses for fake devices, so the lsblk column wins.
    let disks = disks_from_lsblk(&[dev("definitely-not-real-abc", "disk", None)]);
    assert_eq!(disks[0].model.as_deref(), Some("Test Model"));
}

#[test]
fn empty_model_stays_none() {
    let d = LsblkDevice {
        name: "definitely-not-real-abc".into(),
        path: "/dev/definitely-not-real-abc".into(),
        kind: "disk".into(),
        model: Some("   ".into()),
        rm: None,
        ..Default::default()
    };
    let disks = disks_from_lsblk(&[d]);
    assert_eq!(disks[0].model, None);
}
