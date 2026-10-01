use super::*;

#[test]
fn nearby_cache_survives_reopen_and_preserves_newer_sightings() {
    let path = std::env::temp_dir().join(format!(
        "plain-rs-chat-nearby-cache-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    let device = DNearbyDeviceCache {
        id: "phone-1".into(),
        name: "Phone".into(),
        ips: vec!["203.0.113.2".into()],
        port: 8443,
        device_type: "PHONE".into(),
        version: "1".into(),
        platform: "android".into(),
        last_seen: "1970-01-01T00:00:00.100Z".into(),
    };
    let db = Db::open(&path).unwrap();
    db.save_cached_nearby_device(&device).unwrap();
    drop(db);
    let db = Db::open(&path).unwrap();
    assert_eq!(
        db.get_cached_nearby_devices().unwrap(),
        vec![device.clone()]
    );
    let mut updated = device.clone();
    updated.ips = vec!["203.0.113.3".into()];
    let updated = DNearbyDeviceCache {
        last_seen: "1970-01-01T00:00:00.150Z".into(),
        ..updated
    };
    db.save_cached_nearby_device(&updated).unwrap();
    assert!(
        !db.delete_cached_nearby_device_if_last_seen_matches(&device.id, &device.last_seen)
            .unwrap()
    );
    let fresh = db.get_cached_nearby_devices().unwrap().remove(0);
    assert_eq!(fresh.ips, updated.ips);
    assert!(
        db.refresh_cached_nearby_device_if_last_seen_matches(
            &fresh.id,
            &fresh.last_seen,
            "1970-01-01T00:00:00.200Z"
        )
        .unwrap()
    );
    assert!(
        !db.delete_cached_nearby_device_if_last_seen_matches(&fresh.id, &fresh.last_seen)
            .unwrap()
    );
    let latest = db.get_cached_nearby_devices().unwrap().remove(0);
    assert!(
        db.delete_cached_nearby_device_if_last_seen_matches(&latest.id, &latest.last_seen)
            .unwrap()
    );
    assert!(db.get_cached_nearby_devices().unwrap().is_empty());
    drop(db);
    let _ = std::fs::remove_file(path);
}
