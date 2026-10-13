//! NAS assembly of the shared `ChatState` (moved from the plainnas
//! crate's `src/chat` — one chat assembly in plain-rs now).

use super::*;
use std::sync::Arc;

fn unique_tmp_dir(label: &str) -> std::path::PathBuf {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "plain-rs-chat-nas-{label}-{}-{seq}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ))
}

fn make_state(dir: &std::path::Path) -> Arc<ChatState> {
    let prefs = plain_rs::prefs::Prefs::load(&plain_rs::prefs::default_path(dir)).unwrap();
    Arc::new(ChatState::nas_init(dir, &prefs).unwrap())
}

/// nas_init creates the plain-app-schema plain.db under the data dir.
#[test]
fn nas_init_creates_chat_db_with_plain_app_schema() {
    let dir = unique_tmp_dir("init");
    let state = make_state(&dir);

    assert!(dir.join("plain.db").exists());
    let tables = [
        "chats",
        "chat_channels",
        "peers",
        "nearby_device_cache",
        "app_files",
        "tags",
        "audio_queue_items",
        "notes",
        "feeds",
    ];
    for t in tables {
        assert!(
            !state.service.db.table_columns(t).is_empty(),
            "table {t} must exist"
        );
    }
    // Wire device type is NAS.
    assert_eq!(state.service.wire_device_type, DeviceType::Nas);
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
    let prefs = plain_rs::prefs::Prefs::load(&plain_rs::prefs::default_path(&dir)).unwrap();
    let state = ChatState::nas_init(&dir, &prefs).unwrap();
    let url_token = plain_media::storage::kv::UrlToken::new(&prefs)
        .ensure()
        .unwrap();
    assert_eq!(state.service.token, url_token);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn nas_identity_matches_shared_persisted_identity() {
    let directory = tempfile::tempdir().unwrap();
    let prefs =
        plain_rs::prefs::Prefs::load(&plain_rs::prefs::default_path(directory.path())).unwrap();
    let stored = plain_rs::prefs::ensure_identity(&prefs);
    let state = ChatState::nas_init(directory.path(), &prefs).unwrap();
    assert_eq!(state.identity.client_id, stored.client_id);
    assert_eq!(state.identity.device_name(), stored.device_name);
    assert_eq!(state.identity.ed25519_keypair, stored.ed25519_keypair);
}
