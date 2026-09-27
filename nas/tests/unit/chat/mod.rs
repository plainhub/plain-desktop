use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

fn unique_tmp_dir(label: &str) -> std::path::PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "plain-nas-chat-{label}-{}-{seq}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ))
}

fn make_state(dir: &std::path::Path) -> Arc<ChatState> {
    let prefs = crate::prefs::Prefs::load(&crate::prefs::default_path(dir)).unwrap();
    Arc::new(ChatState::init(dir, &prefs).unwrap())
}

/// init creates the plain-app-schema chat.db under the data dir.
#[test]
fn init_creates_chat_db_with_plain_app_schema() {
    let dir = unique_tmp_dir("init");
    let state = make_state(&dir);

    assert!(dir.join("chat.db").exists());
    let tables = [
        "chats",
        "chat_channels",
        "peers",
        "nearby_device_cache",
        "app_files",
    ];
    for t in tables {
        assert!(
            !state.service.db.table_columns(t).is_empty(),
            "table {t} must exist"
        );
    }
    // Wire device type is NAS.
    assert_eq!(
        state.service.wire_device_type,
        plain_rs::chat::enums::DeviceType::Nas
    );
    assert_eq!(state.pairing.local_device_type, "NAS");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Identity is stable across restarts (same prefs dir → same client id +
/// keypair + token) — the pairing identity must not rotate or peers
/// would see a stranger.
#[test]
fn identity_is_stable_across_reopen() {
    let dir = unique_tmp_dir("identity");
    let a = make_state(&dir);
    let b = make_state(&dir);

    assert_eq!(a.service.identity.client_id, b.service.identity.client_id);
    assert_eq!(
        a.service.identity.ed25519_keypair,
        b.service.identity.ed25519_keypair
    );
    assert_eq!(a.service.token, b.service.token);
    // The identity doubles as the /init signature key owner.
    assert!(!a.service.identity.ed25519_keypair.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

/// The service token equals the URL token the /fs endpoint decrypts file
/// ids with — chat attachment ids must resolve through the same path.
#[test]
fn service_token_is_the_fs_url_token() {
    let dir = unique_tmp_dir("token");
    let prefs = crate::prefs::Prefs::load(&crate::prefs::default_path(&dir)).unwrap();
    let state = ChatState::init(&dir, &prefs).unwrap();
    let url_token = crate::db::UrlToken::new(&prefs).ensure().unwrap();
    assert_eq!(state.service.token, url_token);
    let _ = std::fs::remove_dir_all(&dir);
}

/// ureq transport is usable from a single-threaded tokio runtime (the
/// blocking HTTP lives on the blocking pool, not block_in_place).
#[tokio::test]
async fn ureq_transport_runs_on_current_thread_runtime() {
    let t = UreqTransport::new();
    // Documentation-range address: connection fails, mapped to Err —
    // proving the future completes without panicking.
    let res = t
        .post(
            "https://203.0.113.1:1/peer_graphql",
            "c-id-x",
            None,
            b"body",
        )
        .await;
    assert!(res.is_err());
}

/// The event bridge lands chat service events on the `chat:event` bus
/// with the phone-protocol msgType — the shape ws_hub re-frames verbatim.
#[tokio::test]
async fn chat_events_bridge_to_eventbus() {
    use crate::eventbus::EventBus;
    let dir = unique_tmp_dir("bridge");
    let state = make_state(&dir);
    crate::chat::spawn_event_bridge(&state);

    let hits: std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = hits.clone();
    let _sub = EventBus::global().subscribe(crate::consts::EVENT_CHAT, move |p| {
        sink.lock().unwrap().push(p);
    });

    state.service.send_chat_item("local".into(), "{}".into());

    for _ in 0..100 {
        if !hits.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let got = hits.lock().unwrap().clone();
    assert!(!got.is_empty(), "bridge must publish message_created");
    let created = got
        .iter()
        .find(|v| v["msgType"] == 1)
        .expect("WS_MESSAGE_CREATED on the bus");
    let payload = created["payload"].as_str().expect("string payload");
    let body: serde_json::Value = serde_json::from_str(payload).unwrap();
    assert_eq!(body[0]["toId"], "local");
}

#[test]
fn pairing_event_ws_type_matches_phone_protocol() {
    use plain_rs::chat::pairing::PairingEventKind as K;
    use plain_rs::chat::pairing::protocol::PairingRequest;
    let req = PairingRequest {
        from_id: "p".into(),
        from_name: "P".into(),
        port: 1,
        device_type: "PHONE".into(),
        ecdh_public_key: String::new(),
        signature_public_key: String::new(),
        timestamp: 0,
        ips: vec![],
        signature: String::new(),
        aware_supported: false,
        from_ip: String::new(),
    };
    assert_eq!(
        crate::chat::pairing_event_ws_type(&K::IncomingRequest {
            request: Box::new(req),
            sender_ip: String::new()
        }),
        22
    );
    assert_eq!(crate::chat::pairing_event_ws_type(&K::Success), 23);
    assert_eq!(
        crate::chat::pairing_event_ws_type(&K::Failed { reason: "x".into() }),
        24
    );
    assert_eq!(crate::chat::pairing_event_ws_type(&K::Cancelled), 25);
    assert_eq!(crate::chat::pairing_event_ws_type(&K::Started), 26);
}
