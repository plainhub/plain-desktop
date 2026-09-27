//! Unit tests for `src/gql/types.rs` — moved out-of-line; compiled
//! as the `types` child module via `#[cfg(test)] #[path]` there.
use super::AuditEventType;
use super::TrashItemType;

/// Every audit kind the server writes maps to an enum value; anything
/// else (rows written by an older build with a retired kind) must map
/// to `None` so the resolver skips the row instead of erroring.
#[test]
fn event_type_from_kind_covers_all_emitted_kinds() {
    for kind in [
        "login",
        "login_failed",
        "logout",
        "revoke",
        "set_hostname",
        "update_device_name",
        "mount",
        "mount_failed",
        "unmount",
        "format_disk",
        "format_disk_failed",
    ] {
        assert!(AuditEventType::from_kind(kind).is_some(), "{kind} must map");
    }
    assert_eq!(AuditEventType::from_kind("set_device_name"), None);
    assert_eq!(AuditEventType::from_kind(""), None);
}

#[test]
fn trash_item_type_from_kind_is_the_file_dir_closed_set() {
    assert_eq!(TrashItemType::from_kind("file"), Some(TrashItemType::FILE));
    assert_eq!(TrashItemType::from_kind("dir"), Some(TrashItemType::DIR));
    assert_eq!(TrashItemType::from_kind("symlink"), None);
}
