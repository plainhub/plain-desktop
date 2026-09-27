//! Unit tests for `src/mountinfo.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

#[test]
fn mount_matching_uses_path_components() {
    let entries = vec![
        MountInfoEntry {
            mount_point: "/".to_string(),
        },
        MountInfoEntry {
            mount_point: "/Volumes/Data".to_string(),
        },
    ];
    assert_eq!(
        find_best_mount_point(&entries, "/Volumes/Data/photo.jpg"),
        Some("/Volumes/Data".to_string())
    );
    assert_eq!(
        find_best_mount_point(&entries, "/Volumes/Database/photo.jpg"),
        Some("/".to_string())
    );
}

#[test]
fn clean_path_removes_parent_components() {
    assert_eq!(Path::new("/a/../b").clean_path(), PathBuf::from("/b"));
}

#[test]
#[cfg(target_os = "linux")]
fn resolve_root() {
    // Almost any path resolves to some mountpoint.
    let mp = resolve_mount_point("/").unwrap();
    assert!(!mp.is_empty());
}
