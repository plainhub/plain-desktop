use super::*;
use crate::chat::enums::{DeviceType, PeerStatus};
use crate::db::{DPeer, Db};

fn unique_tmp_dir(label: &str) -> std::path::PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "plain-rs-chat-disc-{label}-{}-{seq}",
        std::process::id(),
    ))
}

/// The hostname preference is generated once and persisted — plain-app's
/// `MdnsHostnamePreference.ensureValueAsync` semantics.
#[test]
fn mdns_hostname_is_generated_once_and_persisted() {
    let dir = unique_tmp_dir("hostname");
    let prefs = crate::prefs::Prefs::load(&crate::prefs::default_path(&dir)).unwrap();
    let a = ensure_mdns_hostname(&prefs);
    let b = ensure_mdns_hostname(&prefs);
    assert_eq!(a, b);
    assert!(a.ends_with(".local"));
    assert_eq!(a.trim_end_matches(".local").len(), 2);
    let stored: Option<String> = prefs.get("mdns_hostname").unwrap();
    assert_eq!(stored.as_deref(), Some(a.as_str()));
}

/// `update_known_peer` refreshes paired/logged-in peers' addresses and
/// leaves unrelated rows untouched.
#[test]
fn update_known_peer_refreshes_paired_peers_only() {
    let dir = unique_tmp_dir("update-peer");
    let db = Db::open(&dir.join("plain.db")).unwrap();

    let mut paired = DPeer::new("p1", "Phone", "203.0.113.1", 2443, DeviceType::Phone);
    paired.status = PeerStatus::Paired;
    db.upsert_peer(&paired);
    db.upsert_peer(&DPeer::new(
        "p2",
        "Other",
        "203.0.113.2",
        2443,
        DeviceType::Computer,
    ));

    let device = FoundDevice {
        id: "p1".into(),
        name: "Phone".into(),
        ips: vec!["203.0.113.9".into()],
        ipv6: vec![],
        port: 8443,
        device_type: "PHONE".into(),
        version: "1".into(),
        platform: "android".into(),
    };
    update_known_peer(&db, &device);
    let updated = db.get_peer_by_id("p1").unwrap();
    assert_eq!(updated.ip, "203.0.113.9");
    assert_eq!(updated.port, 8443);

    let device2 = FoundDevice {
        id: "p2".into(),
        ..device
    };
    update_known_peer(&db, &device2);
    // p2 is neither paired nor logged in → untouched.
    assert_eq!(db.get_peer_by_id("p2").unwrap().ip, "203.0.113.2");
}
