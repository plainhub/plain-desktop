//! Unit tests for `src/mounts.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

#[test]
fn classify_keeps_block_mount() {
    let ml = classify_mount_line("/dev/sda1 / ext4 rw,relatime 0 0").unwrap();
    assert_eq!(ml.src, "/dev/sda1");
    assert_eq!(ml.target, "/");
    assert!(ml.is_block);
    assert!(!ml.is_remote);
}

#[test]
fn classify_skips_pseudo_filesystems() {
    for fstype in [
        "proc", "sysfs", "tmpfs", "devtmpfs", "cgroup2", "overlay", "squashfs",
    ] {
        assert!(
            classify_mount_line(&format!("/dev/anything /mnt/x {fstype} rw 0 0")).is_none(),
            "{fstype} should be skipped"
        );
    }
}

#[test]
fn classify_skips_system_mountpoints() {
    for target in ["/boot", "/boot/efi", "/efi", "/boot/firmware"] {
        assert!(
            classify_mount_line(&format!("/dev/sda1 {target} vfat rw 0 0")).is_none(),
            "{target} should be skipped"
        );
    }
}

#[test]
fn classify_skips_bind_mounts() {
    assert!(classify_mount_line("/dev/sda1 /mnt/bind ext4 rw,bind 0 0").is_none());
    assert!(classify_mount_line("/dev/sda1 /mnt/bind ext4 rw,relatime,bind 0 0").is_none());
    // Non-bind option lists survive.
    assert!(classify_mount_line("/dev/sda1 /mnt/x ext4 rw,relatime 0 0").is_some());
}

#[test]
fn classify_skips_non_storage_sources() {
    // Neither block (/dev/) nor remote.
    assert!(classify_mount_line("ahlsec /mnt/x ahlsec rw 0 0").is_none());
}

#[test]
fn classify_skips_zram_and_loop() {
    assert!(classify_mount_line("/dev/zram0 /swap swap rw 0 0").is_none());
    assert!(classify_mount_line("/dev/loop0 /mnt/x squashfs ro 0 0").is_none());
}

#[test]
fn classify_detects_remote_mounts() {
    // src contains ':' (NFS-style).
    let ml = classify_mount_line("nas:/export /mnt/nfs nfs4 rw 0 0").unwrap();
    assert!(ml.is_remote);
    assert!(!ml.is_block);
    // cifs/smb/sshfs fstypes with a // src.
    let ml = classify_mount_line("//192.168.1.5/share /mnt/smb cifs rw 0 0").unwrap();
    assert!(ml.is_remote);
    assert!(!ml.is_block);
    let ml = classify_mount_line("user@host:/path /mnt/ssh sshfs rw 0 0").unwrap();
    assert!(ml.is_remote);
}

#[test]
fn classify_short_lines_dropped() {
    assert!(classify_mount_line("").is_none());
    assert!(classify_mount_line("/dev/sda1 /mnt").is_none());
}

#[test]
fn non_empty_helper() {
    assert_eq!(non_empty("  "), None);
    assert_eq!(non_empty(""), None);
    assert_eq!(non_empty("photos"), Some("photos".to_string()));
}
