use super::*;

#[test]
fn main_window_cannot_open_before_startup_gate() {
    let config: plain_libs::serde_json::Value =
        plain_libs::serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();
    assert_eq!(config["app"]["windows"][0]["create"].as_bool(), Some(false));
}

#[test]
fn only_pending_legacy_installations_show_the_gate() {
    let data = plain_libs::temporary::tempdir().unwrap();
    let root = data.path();
    assert!(!pending(root));
    std::fs::write(root.join("local_chat.db"), b"legacy").unwrap();
    assert!(pending(root));
    let db = Connection::open(DataPaths::new(root).database()).unwrap();
    db.execute_batch(
        "CREATE TABLE key_values(key TEXT PRIMARY KEY, value TEXT NOT NULL);
        INSERT INTO key_values VALUES ('desktop_chat_upgrade_4_0_0', 'rows_imported');",
    )
    .unwrap();
    assert!(pending(root));
    db.execute("UPDATE key_values SET value = 'complete'", [])
        .unwrap();
    assert!(!pending(root));
}

#[test]
fn migration_window_navigation_cannot_select_a_remote_origin() {
    let dev = Url::parse("http://localhost:4000").unwrap();
    for uri in [
        "tauri://localhost/desktop-migration.html",
        "http://tauri.localhost/desktop-migration.html?retry=1",
        "http://localhost:4000/desktop-migration.html",
    ] {
        assert!(page(&Url::parse(uri).unwrap(), Some(&dev)));
    }
    for uri in [
        "https://other.example/desktop-migration.html?retry=1",
        "http://tauri.localhost:9999/desktop-migration.html?retry=1",
        "tauri://localhost/about?retry=1",
    ] {
        assert!(!page(&Url::parse(uri).unwrap(), Some(&dev)));
    }
}

#[tokio::test]
async fn startup_stays_blocked_after_failure_and_can_retry_the_same_installation() {
    let fixture = plain_libs::temporary::tempdir().unwrap();
    let data = fixture.path().join("data");
    let cache = fixture.path().join("cache");
    std::fs::create_dir(&data).unwrap();
    let legacy = data.join("local_chat.db");
    let prefs = data.join("prefs.json");
    let old_prefs = br#"{"client_id":"plain1","device_name":"Fixture"}"#;
    std::fs::write(&legacy, b"not a SQLite file").unwrap();
    std::fs::write(&prefs, old_prefs).unwrap();

    assert!(pending(&data));
    assert!(crate::bootstrap::open_desktop_core(&data, &cache).is_err());
    assert!(pending(&data));
    assert_eq!(std::fs::read(&legacy).unwrap(), b"not a SQLite file");
    assert_eq!(std::fs::read(&prefs).unwrap(), old_prefs);

    // Restore a valid schema in the test fixture and retry the same startup path.
    std::fs::remove_file(&legacy).unwrap();
    let old = Connection::open(legacy).unwrap();
    old.execute_batch(
        "CREATE TABLE chats (id TEXT, from_id TEXT, to_id TEXT, channel_id TEXT, content TEXT, status TEXT, status_data TEXT, created_at TEXT, updated_at TEXT);
         CREATE TABLE chat_channels (id TEXT, name TEXT, owner TEXT, members TEXT, key TEXT, version INTEGER, status TEXT, created_at TEXT, updated_at TEXT);
         CREATE TABLE peers (id TEXT, name TEXT, ip TEXT, key TEXT, public_key TEXT, status TEXT, port INTEGER, device_type TEXT, token TEXT, created_at TEXT, updated_at TEXT);
         CREATE TABLE app_files (id TEXT, size INTEGER, mime_type TEXT, real_path TEXT, ref_count INTEGER, weak_hash TEXT, created_at TEXT, updated_at TEXT);",
    ).unwrap();
    old.execute(
        "INSERT INTO chats VALUES ('00abcdef12345678', 'me', 'local', '', ?1, 'SENT', '', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        [r#"{"type":"text","value":{"text":"Legacy Desk"}} "#],
    ).unwrap();
    drop(old);
    let core = crate::bootstrap::open_desktop_core(&data, &cache).unwrap();
    assert!(!pending(&data));
    assert_eq!(
        core.native_graphql().unwrap().client_id(),
        "706c6169-0001-8000-8000-00005c3bb5dd"
    );
    core.stop_core().await;
}
