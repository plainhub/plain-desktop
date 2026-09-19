//! Unit tests for `src-tauri/src/local/graphql/schema/app.rs`.
//! Locked contracts: pmset battery parsing, Windows power-status mapping,
//! thermal zone reading, volume mount selection.

use std::path::{Path, PathBuf};

use super::*;

fn temp_fixture_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("plain-desktop-devstatus-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn pmset_parse_charging() {
    let out = "Now drawing from 'AC Power'\n -InternalBattery-0 (id=123)	64%; charging; 2:11 remaining\n";
    assert_eq!(parse_pmset_battery(out), Some((64, true)));
}

#[test]
fn pmset_parse_discharging_is_not_charging() {
    // "discharging" contains the substring "charging" — must not flip the flag.
    let out = "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=123)	87%; discharging; 3:28 remaining\n";
    assert_eq!(parse_pmset_battery(out), Some((87, false)));
}

#[test]
fn pmset_parse_full_while_plugged_is_not_charging() {
    let out = "Now drawing from 'AC Power'\n -InternalBattery-0 (id=123)	100%; charged; 0:00 remaining\n";
    assert_eq!(parse_pmset_battery(out), Some((100, false)));
    let out = "Now drawing from 'AC Power'\n -InternalBattery-0 (id=123)	99%; finishing charge\n";
    assert_eq!(parse_pmset_battery(out), Some((99, false)));
}

#[test]
fn pmset_parse_desktop_without_battery_is_none() {
    assert_eq!(parse_pmset_battery("Now drawing from 'AC Power'\n"), None);
    assert_eq!(parse_pmset_battery(""), None);
}

#[test]
fn win_battery_maps_flags() {
    // No battery (128) / unknown percent (255) / out of range → None.
    assert_eq!(win_battery_from(128, 80), None);
    assert_eq!(win_battery_from(1, 255), None);
    assert_eq!(win_battery_from(1, 101), None);
    // Charging flag 8.
    assert_eq!(win_battery_from(8, 55), Some((55, true)));
    // High/low/critical flags without 8 → not charging.
    assert_eq!(win_battery_from(1, 55), Some((55, false)));
    assert_eq!(win_battery_from(0, 100), Some((100, false)));
}

#[test]
fn thermal_zones_read_type_and_millidegrees() {
    let root = temp_fixture_dir("thermal");
    for (zone, label, milli) in [("zone1", "cpu-thermal", "45000"), ("zone2", "gpu-thermal", "38500")] {
        let dir = root.join(format!("thermal_{}", zone));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("type"), label).unwrap();
        std::fs::write(dir.join("temp"), milli).unwrap();
    }
    // A zone with an invalid temp and an unrelated entry are both skipped.
    let broken = root.join("thermal_zone3");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::write(broken.join("type"), "broken").unwrap();
    std::fs::write(broken.join("temp"), "not-a-number").unwrap();
    std::fs::create_dir_all(root.join("not_a_zone")).unwrap();

    let temps = thermal_zones(&root);
    assert_eq!(temps.len(), 2);
    assert_eq!(temps[0].label, "cpu-thermal");
    assert_eq!(temps[0].celsius, 45.0);
    assert_eq!(temps[1].label, "gpu-thermal");
    assert_eq!(temps[1].celsius, 38.5);

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn thermal_zones_missing_root_is_empty() {
    assert!(thermal_zones(Path::new("/nonexistent-plain-desktop-test")).is_empty());
}

#[test]
fn longest_prefix_mount_prefers_most_specific() {
    let mounts = [
        Path::new("/"),
        Path::new("/Volumes/Data"),
        Path::new("/Volumes/Data/projects"),
    ];
    assert_eq!(
        longest_prefix_mount(&mounts, Path::new("/Volumes/Data/projects/a/b")),
        Some(Path::new("/Volumes/Data/projects"))
    );
    assert_eq!(
        longest_prefix_mount(&mounts, Path::new("/Volumes/Data/x")),
        Some(Path::new("/Volumes/Data"))
    );
    assert_eq!(longest_prefix_mount(&mounts, Path::new("/etc/hosts")), Some(Path::new("/")));
    assert_eq!(longest_prefix_mount(&mounts, Path::new("/other/absent")), Some(Path::new("/")));
}

#[test]
fn longest_prefix_mount_no_match_is_none() {
    let mounts = [Path::new("/Volumes/Data")];
    assert_eq!(longest_prefix_mount(&mounts, Path::new("/Users/x")), None);
    assert_eq!(longest_prefix_mount(&[], Path::new("/Users/x")), None);
}
