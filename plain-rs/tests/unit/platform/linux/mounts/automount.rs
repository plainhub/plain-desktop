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

    props.insert("DEVTYPE".to_string(), "scsi_device".to_string());
    assert!(!should_trigger_hotplug(&props));

    props.insert("DEVTYPE".to_string(), "disk".to_string());
    props.insert("SUBSYSTEM".to_string(), "net".to_string());
    assert!(!should_trigger_hotplug(&props));
    props.insert("SUBSYSTEM".to_string(), "block".to_string());
    for action in ["online", "move", ""] {
        props.insert("ACTION".to_string(), action.to_string());
        assert!(!should_trigger_hotplug(&props), "action {action:?}");
    }
    props.insert("ACTION".to_string(), "change".to_string());
    props.remove("SUBSYSTEM");
    props.remove("DEVTYPE");
    assert!(should_trigger_hotplug(&props));
}

#[test]
fn inhibition_is_released_after_error_and_nested_guards() {
    let before = AUTO_MOUNT_INHIBIT_COUNT.load(Ordering::SeqCst);
    let guard = inhibit();
    let fail = || -> anyhow::Result<()> {
        let _nested = inhibit();
        anyhow::bail!("operation failed")
    };
    assert!(fail().is_err());
    assert_eq!(AUTO_MOUNT_INHIBIT_COUNT.load(Ordering::SeqCst), before + 1);
    drop(guard);
    assert_eq!(AUTO_MOUNT_INHIBIT_COUNT.load(Ordering::SeqCst), before);
}

#[tokio::test]
async fn watcher_shutdown_reaps_the_monitor_without_output() {
    let directory = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&directory.path().join("prefs.json")).unwrap());
    let events: EventSink = Arc::new(|_, _| panic!("no mount should run"));
    let child = tokio::process::Command::new("sleep")
        .arg("60")
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = child.id().unwrap();
    let (stop, receiver) = tokio::sync::watch::channel(false);
    let task = tokio::spawn(watch_child(child, prefs, events, receiver));
    stop.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(unsafe { libc::kill(pid as i32, 0) }, -1);
}

#[tokio::test]
async fn monitor_exit_is_reported_as_a_failure() {
    let directory = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&directory.path().join("prefs.json")).unwrap());
    let events: EventSink = Arc::new(|_, _| panic!("no mount should run"));
    let child = tokio::process::Command::new("true")
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let (_stop, receiver) = tokio::sync::watch::channel(false);
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        watch_child(child, prefs, events, receiver),
    )
    .await
    .unwrap();
    assert!(result.unwrap_err().to_string().contains("monitor stopped"));
}
