//! Unit tests for `src/format_disk.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

#[test]
fn first_partition_sata() {
    assert_eq!(first_partition_path("/dev/sda"), "/dev/sda1");
    assert_eq!(first_partition_path("/dev/sdb"), "/dev/sdb1");
}

#[test]
fn first_partition_nvme() {
    assert_eq!(first_partition_path("/dev/nvme0n1"), "/dev/nvme0n1p1");
    assert_eq!(first_partition_path("/dev/nvme1n2"), "/dev/nvme1n2p1");
}

#[test]
fn first_partition_mmc() {
    assert_eq!(first_partition_path("/dev/mmcblk0"), "/dev/mmcblk0p1");
}

#[test]
fn first_partition_empty() {
    assert_eq!(first_partition_path(""), "");
}

#[test]
fn require_tool_missing_returns_err() {
    assert!(require_tool("__definitely_not_a_real_tool_xyz__").is_err());
}
