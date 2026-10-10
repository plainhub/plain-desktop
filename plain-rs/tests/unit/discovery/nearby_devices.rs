use super::*;
fn device(id: &str, method: &str) -> Device {
    Device {
        id: id.into(),
        name: "Fixture".into(),
        ips: vec!["10.0.0.1".into()],
        port: 2443,
        device_type: "PHONE".into(),
        version: "1".into(),
        platform: "test".into(),
        last_seen: crate::db::now_iso(),
        discovery_methods: vec![method.into()],
    }
}
#[test]
fn merges_methods_and_ips_with_monotonic_event_throttle() {
    let devices = Devices::default();
    let t = Instant::now();
    assert!(devices.seen_at(device("one", "BLE"), t).unwrap());
    let mut next = device("one", "LAN");
    next.ips.push("10.0.0.2".into());
    assert!(
        !devices
            .seen_at(next.clone(), t + Duration::from_millis(999))
            .unwrap()
    );
    assert!(devices.seen_at(next, t + Duration::from_secs(1)).unwrap());
    assert!(!devices.observe_at(device("two", "LAN"), t, false).unwrap());
    assert!(devices.seen_at(device("two", "LAN"), t).unwrap());
    devices.forget("two");
    let snapshot = devices.snapshot();
    assert_eq!(snapshot.devices.len(), 1);
    assert_eq!(snapshot.devices[0].ips, vec!["10.0.0.1", "10.0.0.2"]);
    assert_eq!(snapshot.devices[0].discovery_methods, vec!["BLE", "LAN"]);
}
#[test]
fn stale_probe_cannot_remove_a_new_sighting_or_new_cached_version() {
    let dir = tempfile::tempdir().unwrap();
    let db = crate::db::Db::open(&dir.path().join("test.db")).unwrap();
    let devices = Devices::default();
    devices.scanning(true, false, vec![]).unwrap();
    let t = Instant::now();
    devices.seen_at(device("one", "LAN"), t).unwrap();
    let old = devices
        .stale_at(&db, t + Duration::from_secs(61))
        .unwrap()
        .pop()
        .unwrap();
    devices
        .seen_at(device("one", "BLE"), t + Duration::from_secs(62))
        .unwrap();
    assert!(!devices.verified(&db, &old, false).unwrap());
    assert_eq!(devices.snapshot().devices.len(), 1);
    let stale = devices
        .stale_at(&db, t + Duration::from_secs(124))
        .unwrap()
        .pop()
        .unwrap();
    let mut newer = stale.device.cache();
    newer.last_seen = "2099-01-01T00:00:00Z".into();
    crate::db::chat_store::nearby::save(&db, &newer).unwrap();
    assert!(!devices.verified(&db, &stale, false).unwrap());
    assert_eq!(crate::db::chat_store::nearby::all(&db).unwrap().len(), 1);
    crate::db::chat_store::nearby::delete(&db, "one").unwrap();
    let stale = devices
        .stale_at(&db, t + Duration::from_secs(124))
        .unwrap()
        .pop()
        .unwrap();
    assert!(devices.verified(&db, &stale, false).unwrap());
    assert!(devices.snapshot().devices.is_empty());
}
#[test]
fn stopped_scan_cannot_apply_pending_verdict_and_registry_is_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let db = crate::db::Db::open(&dir.path().join("test.db")).unwrap();
    let devices = Devices::default();
    devices.scanning(true, false, vec![]).unwrap();
    let t = Instant::now();
    for i in 0..513 {
        devices
            .seen_at(device(&i.to_string(), "BLE"), t + Duration::from_millis(i))
            .unwrap();
    }
    assert_eq!(devices.snapshot().devices.len(), 512);
    assert!(!devices.snapshot().devices.iter().any(|d| d.id == "0"));
    let stale = devices
        .stale_at(&db, t + Duration::from_secs(62))
        .unwrap()
        .pop()
        .unwrap();
    devices.scanning(false, false, vec![]).unwrap();
    assert!(!devices.verified(&db, &stale, false).unwrap());
    devices.scanning(true, false, vec![]).unwrap();
    assert!(!devices.verified(&db, &stale, false).unwrap());
    devices.scanning(false, false, vec![]).unwrap();
    assert!(
        devices
            .stale_at(&db, t + Duration::from_secs(63))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn merged_ble_timestamp_still_allows_checked_lan_cache_cleanup() {
    let dir = tempfile::tempdir().unwrap();
    let db = crate::db::Db::open(&dir.path().join("test.db")).unwrap();
    let devices = Devices::default();
    devices.scanning(true, false, vec![]).unwrap();
    let t = Instant::now();
    let lan = device("one", "LAN");
    crate::db::chat_store::nearby::save(&db, &lan.cache()).unwrap();
    devices.seen_at(lan, t).unwrap();
    let mut ble = device("one", "BLE");
    ble.last_seen = "2099-01-01T00:00:00Z".into();
    devices.seen_at(ble, t + Duration::from_secs(1)).unwrap();
    let stale = devices
        .stale_at(&db, t + Duration::from_secs(62))
        .unwrap()
        .pop()
        .unwrap();
    assert!(devices.verified(&db, &stale, false).unwrap());
    assert!(crate::db::chat_store::nearby::all(&db).unwrap().is_empty());
}
