//! Unit tests for `src/automount.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

fn fs(uuid: &str, mountpoint: &str) -> BlockFs {
    BlockFs {
        path: format!("/dev/disk-{uuid}"),
        fsuuid: uuid.to_string(),
        fstype: "ext4".to_string(),
        size_bytes: 0,
        mountpoint: mountpoint.to_string(),
    }
}

fn slots(pairs: &[(&str, i64)]) -> SlotMap {
    SlotMap(pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect())
}

#[test]
fn parse_usb_slot_valid() {
    assert_eq!(parse_usb_slot("/mnt/usb1"), Some(1));
    assert_eq!(parse_usb_slot("/mnt/usb12"), Some(12));
}

#[test]
fn parse_usb_slot_invalid() {
    assert_eq!(parse_usb_slot(""), None);
    assert_eq!(parse_usb_slot("/mnt/usb"), None);
    assert_eq!(parse_usb_slot("/mnt/usba"), None);
    assert_eq!(parse_usb_slot("/mnt/usb1x"), None);
    assert_eq!(parse_usb_slot("/mnt/external"), None);
    assert_eq!(parse_usb_slot("/media/usb1"), None);
    assert_eq!(parse_usb_slot("/mnt/usb"), None);
}

#[test]
fn next_free_slot_picks_smallest() {
    let used: HashSet<usize> = [1, 2, 4].into_iter().collect();
    assert_eq!(next_free_slot(&used), 3);
    assert_eq!(next_free_slot(&HashSet::new()), 1);
}

#[test]
fn plan_mounts_unmounted_fs_gets_persisted_slot() {
    let persisted = slots(&[("uuid-a", 3)]);
    let (planned, merged) = plan_mounts(&[fs("uuid-a", "")], &persisted);
    assert_eq!(
        planned,
        vec![PlannedMount {
            fsuuid: "uuid-a".into(),
            slot: 3
        }]
    );
    assert_eq!(merged.0.get("uuid-a"), Some(&3));
}

#[test]
fn plan_mounts_does_not_steal_occupied_slot() {
    // uuid-b currently mounted at /mnt/usb2; uuid-a's persisted slot 2 is
    // occupied by another filesystem → uuid-a must move to a free slot.
    let persisted = slots(&[("uuid-a", 2), ("uuid-b", 2)]);
    let filesystems = vec![fs("uuid-b", "/mnt/usb2"), fs("uuid-a", "")];
    let (planned, merged) = plan_mounts(&filesystems, &persisted);
    assert_eq!(
        planned,
        vec![PlannedMount {
            fsuuid: "uuid-a".into(),
            slot: 1
        }]
    );
    assert_eq!(merged.0.get("uuid-a"), Some(&1));
    assert_eq!(merged.0.get("uuid-b"), Some(&2));
}

#[test]
fn plan_mounts_keeps_reservations_for_unplugged() {
    // uuid-gone is persisted at slot 5 but absent; a new filesystem must
    // not take slot 5, and the reservation must survive the merge.
    let persisted = slots(&[("uuid-gone", 5)]);
    let (planned, merged) = plan_mounts(&[fs("uuid-new", "")], &persisted);
    assert_eq!(planned[0].slot, 1);
    assert_eq!(merged.0.get("uuid-gone"), Some(&5));
    assert_eq!(merged.0.get("uuid-new"), Some(&1));
}

#[test]
fn plan_mounts_already_mounted_anywhere_not_planned() {
    let filesystems = vec![fs("uuid-a", "/srv/data")];
    let (planned, merged) = plan_mounts(&filesystems, &SlotMap::default());
    assert!(planned.is_empty());
    assert!(merged.0.is_empty());
}

#[test]
fn plan_mounts_multiple_unmounted_get_sequential_slots() {
    let filesystems = vec![fs("uuid-a", ""), fs("uuid-b", "")];
    let (planned, merged) = plan_mounts(&filesystems, &SlotMap::default());
    let mut slots: Vec<usize> = planned.iter().map(|p| p.slot).collect();
    slots.sort();
    assert_eq!(slots, vec![1, 2]);
    assert_eq!(merged.0.len(), 2);
}

#[test]
fn plan_mounts_skips_empty_uuid() {
    let filesystems = vec![fs("", "")];
    let (planned, merged) = plan_mounts(&filesystems, &SlotMap::default());
    assert!(planned.is_empty());
    assert!(merged.0.is_empty());
}

#[test]
fn plan_mounts_invalid_persisted_slots_ignored() {
    // slot <= 0 is not a valid reservation.
    let persisted = slots(&[("uuid-a", 0), ("uuid-b", -3)]);
    let (planned, merged) = plan_mounts(&[fs("uuid-a", ""), fs("uuid-b", "")], &persisted);
    let mut slots: Vec<usize> = planned.iter().map(|p| p.slot).collect();
    slots.sort();
    assert_eq!(slots, vec![1, 2]);
    assert_eq!(merged.0.get("uuid-a"), Some(&1));
    assert_eq!(merged.0.get("uuid-b"), Some(&2));
}

#[test]
fn hotplug_trigger_rules() {
    let mut props = HashMap::new();
    assert!(!should_trigger_hotplug(&props));
    props.insert("ACTION".to_string(), "add".to_string());
    props.insert("SUBSYSTEM".to_string(), "block".to_string());
    props.insert("DEVTYPE".to_string(), "disk".to_string());
    assert!(should_trigger_hotplug(&props));

    props.insert("DEVTYPE".to_string(), "partition".to_string());
    assert!(should_trigger_hotplug(&props));

    // Unknown device types are ignored.
    props.insert("DEVTYPE".to_string(), "scsi_device".to_string());
    assert!(!should_trigger_hotplug(&props));

    props.insert("DEVTYPE".to_string(), "disk".to_string());
    // Wrong subsystem.
    props.insert("SUBSYSTEM".to_string(), "net".to_string());
    assert!(!should_trigger_hotplug(&props));
    props.insert("SUBSYSTEM".to_string(), "block".to_string());
    // Actions other than add/remove/change are ignored.
    for action in ["online", "move", ""] {
        props.insert("ACTION".to_string(), action.to_string());
        assert!(!should_trigger_hotplug(&props), "action {action:?}");
    }
    // Empty subsystem/DEVTYPE default to acceptable.
    props.insert("ACTION".to_string(), "change".to_string());
    props.remove("SUBSYSTEM");
    props.remove("DEVTYPE");
    assert!(should_trigger_hotplug(&props));
}
