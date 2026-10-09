use super::*;
#[tokio::test]
async fn rust_sweep_honors_radio_pause_and_real_tls_verdicts() {
    use std::sync::Arc;
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap());
    prefs.set("client_id", "local").unwrap();
    let server = super::super::ContentServer::start(
        &dir.path().join("data.db"),
        &crate::base64_encode(&[3; 32]),
        prefs,
    )
    .unwrap();
    let state = server.runtime_state();
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let listeners = crate::http_transport::HttpListeners::start(
        axum::Router::new().route("/nearby", axum::routing::post(|| async { "1" })),
        0,
        0,
        cert.cert.pem().into_bytes(),
        cert.key_pair.serialize_pem().into_bytes(),
    )
    .await
    .unwrap();
    let fixture = |id: &str, port| Device {
        id: id.into(),
        name: "fixture".into(),
        ips: vec!["127.0.0.1".into()],
        port,
        device_type: "PHONE".into(),
        version: "1".into(),
        platform: "test".into(),
        last_seen: "1970-01-01T00:00:00Z".into(),
        discovery_methods: vec!["LAN".into()],
    };
    for d in [fixture("alive", listeners.https_port), fixture("dead", 0)] {
        execute(
            &state,
            Request::Seen {
                device: d,
                resident: true,
                visible: false,
            },
        )
        .await
        .unwrap();
    }
    execute(
        &state,
        Request::Seen {
            device: fixture("local", 2443),
            resident: true,
            visible: true,
        },
    )
    .await
    .unwrap();
    assert_eq!(chat_store::nearby::all(&state.db).unwrap().len(), 2);
    execute(&state, Request::BleScanning { ble: true })
        .await
        .unwrap();
    let (generation, mut requests) = state.host.connect();
    let host = state.host.clone();
    let facts = tokio::spawn(async move {
        for paused in [true, false] {
            let request = requests.recv().await.unwrap();
            assert_eq!(request["method"], "nearbyScanFacts");
            host.reply(
                generation,
                json!({"id":request["id"],"result":{"paused":paused,"interfaces":[]}}),
            )
            .unwrap();
        }
    });
    sweep(&state).await.unwrap();
    assert_eq!(state.nearby_devices.snapshot().devices.len(), 2);
    sweep(&state).await.unwrap();
    facts.await.unwrap();
    assert_eq!(state.nearby_devices.snapshot().devices.len(), 1);
    assert_eq!(state.nearby_devices.snapshot().devices[0].id, "alive");
    assert_eq!(chat_store::nearby::all(&state.db).unwrap().len(), 1);
    assert!(state.nearby_devices.stale(&state.db).unwrap().is_empty());
    let mut peer = crate::db::DPeer::new(
        "live",
        "old",
        "",
        2443,
        crate::chat::enums::DeviceType::Phone,
    );
    peer.status = crate::chat::enums::PeerStatus::Paired;
    peer.key = "preserved-key".into();
    chat_store::peers::save(&state.db, &[peer], chat_store::SaveMode::Insert).unwrap();
    let mut events = state.events.subscribe();
    execute(
        &state,
        Request::Seen {
            device: fixture("live", 2443),
            resident: true,
            visible: true,
        },
    )
    .await
    .unwrap();
    let snapshot = execute(&state, Request::Snapshot).await.unwrap();
    assert_eq!(
        snapshot["devices"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["id"] == "live")
            .unwrap()["status"],
        "PAIRED"
    );
    let peer = chat_store::peers::get(&state.db, "live").unwrap().unwrap();
    assert_eq!(peer.key, "preserved-key");
    assert_eq!(peer.name, "fixture");
    let invalidation = events.recv().await.unwrap();
    assert_eq!(invalidation.event_type, EVENT_UPDATED);
    let first = events.recv().await.unwrap();
    assert_eq!(
        first.event_type,
        crate::chat::events::WS_NEARBY_DEVICE_FOUND
    );
    let public: serde_json::Value = serde_json::from_str(&first.payload).unwrap();
    assert_eq!(public["id"], "live");
    assert_eq!(public["status"], "PAIRED");
    assert!(public.get("key").is_none());
    execute(
        &state,
        Request::Seen {
            device: fixture("live", 2443),
            resident: false,
            visible: true,
        },
    )
    .await
    .unwrap();
    let next = events.recv().await.unwrap();
    assert_eq!(next.event_type, EVENT_UPDATED);
    assert!(events.try_recv().is_err());
    listeners.shutdown().await;
    server.shutdown().await;
}

#[tokio::test]
async fn nearby_invalidation_and_public_device_payloads_have_distinct_contracts() {
    let dir = tempfile::tempdir().unwrap();
    let prefs =
        std::sync::Arc::new(crate::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap());
    prefs.set("client_id", "local").unwrap();
    let server = super::super::ContentServer::start(
        &dir.path().join("data.db"),
        &crate::base64_encode(&[3; 32]),
        prefs,
    )
    .unwrap();
    let state = server.runtime_state();
    let mut events = state.events.subscribe();
    let device = Device {
        id: "synthetic-nearby".into(),
        name: "Synthetic".into(),
        ips: vec![],
        port: 2443,
        device_type: "PHONE".into(),
        version: "1".into(),
        platform: "test".into(),
        last_seen: "2026-10-10T00:00:00Z".into(),
        discovery_methods: vec!["BLE".into()],
    };
    execute(
        &state,
        Request::Seen {
            device: device.clone(),
            visible: true,
            resident: false,
        },
    )
    .await
    .unwrap();
    let invalidation = events.recv().await.unwrap();
    assert_eq!(invalidation.event_type, EVENT_UPDATED);
    let notice: serde_json::Value = serde_json::from_str(&invalidation.payload).unwrap();
    assert!(notice["revision"].is_u64());
    assert!(notice.get("id").is_none());
    let public = events.recv().await.unwrap();
    assert_eq!(
        public.event_type,
        crate::chat::events::WS_NEARBY_DEVICE_FOUND
    );
    let row: serde_json::Value = serde_json::from_str(&public.payload).unwrap();
    assert_eq!(row["id"], device.id);
    assert_eq!(row["name"], device.name);
    assert_eq!(row["status"], "UNPAIRED");
    assert_eq!(row["bestIp"], "");
    assert_eq!(row["discoveryMethods"], json!(["BLE"]));
    assert!(row.get("eventId").is_none());
    execute(&state, Request::Snapshot).await.unwrap();
    assert!(events.try_recv().is_err());
    execute(
        &state,
        Request::Seen {
            device,
            visible: true,
            resident: false,
        },
    )
    .await
    .unwrap();
    assert_eq!(events.recv().await.unwrap().event_type, EVENT_UPDATED);
    assert!(events.try_recv().is_err());
    server.shutdown().await;
}
