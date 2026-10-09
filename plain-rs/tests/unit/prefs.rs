//! Unit tests for `src/prefs/mod.rs` — compiled as the `tests` child
//! module via `#[cfg(test)] #[path]` there.
use super::*;

fn tmp_prefs(tag: &str) -> (std::path::PathBuf, Prefs) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "plain-rs-prefs-{tag}-{}-{nanos}",
        std::process::id()
    ));
    let path = dir.join("system_prefs.json");
    (dir.clone(), Prefs::load(&path).unwrap())
}

#[test]
fn missing_file_starts_empty_and_creates_on_first_set() {
    let (dir, prefs) = tmp_prefs("missing");
    assert_eq!(prefs.get::<String>("device_name").unwrap(), None);
    assert!(prefs.set("device_name", "box").unwrap());
    let file = dir.join("system_prefs.json");
    assert!(file.exists(), "file created on first set");
    assert!(
        !dir.join("system_prefs.json.tmp").exists(),
        "tmp file renamed away"
    );
}

#[test]
fn roundtrip_string_and_json_shapes() {
    let (_dir, prefs) = tmp_prefs("roundtrip");
    prefs.set("url_token", "abc123").unwrap();
    prefs.set("recent_files", vec!["/a", "/b"]).unwrap();
    prefs
        .set("samba", serde_json::json!({"enabled": true}))
        .unwrap();

    assert_eq!(
        prefs.get::<String>("url_token").unwrap().as_deref(),
        Some("abc123")
    );
    assert_eq!(
        prefs.get::<Vec<String>>("recent_files").unwrap(),
        Some(vec!["/a".into(), "/b".into()])
    );
    assert_eq!(
        prefs.get::<serde_json::Value>("samba").unwrap(),
        Some(serde_json::json!({"enabled": true}))
    );
}

#[test]
fn system_and_user_preferences_are_persisted_separately() {
    let (dir, prefs) = tmp_prefs("split-stores");
    prefs.set("theme", "system").unwrap();
    prefs
        .set_user("theme", serde_json::json!({"mode": "dark"}))
        .unwrap();

    let system: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("system_prefs.json")).unwrap())
            .unwrap();
    let user: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("user_prefs.json")).unwrap())
            .unwrap();
    assert_eq!(system["theme"], "system");
    assert_eq!(user["theme"], serde_json::json!({"mode": "dark"}));
    assert_eq!(
        prefs.get_user::<serde_json::Value>("theme").unwrap(),
        Some(serde_json::json!({"mode": "dark"}))
    );
    assert_eq!(
        prefs.get::<String>("theme").unwrap().as_deref(),
        Some("system")
    );
}

#[test]
fn user_store_has_matching_default_and_clear_operations() {
    let (dir, prefs) = tmp_prefs("user-ops");
    assert_eq!(prefs.get_user_or("missing", 42), 42);
    prefs.set("system", true).unwrap();
    prefs.set_user("user", "value").unwrap();
    prefs.clear_user().unwrap();
    assert!(prefs.user_entries().is_empty());
    assert!(prefs.get_or("system", false));
    let reloaded = Prefs::load(&default_path(&dir)).unwrap();
    assert!(reloaded.user_entries().is_empty());
    assert!(reloaded.get_or("system", false));
}

#[test]
fn set_same_value_skips_disk_write() {
    let (dir, prefs) = tmp_prefs("noop");
    assert!(prefs.set("k", 1u32).unwrap());
    let file = dir.join("system_prefs.json");
    let first = std::fs::read(&file).unwrap();
    // Same value again: no rewrite reported.
    assert!(!prefs.set("k", 1u32).unwrap());
    assert_eq!(std::fs::read(&file).unwrap(), first);
    // Different value: rewritten.
    assert!(prefs.set("k", 2u32).unwrap());
    assert_ne!(std::fs::read(&file).unwrap(), first);
}

#[test]
fn remove_persists_and_reports_presence() {
    let (_dir, prefs) = tmp_prefs("remove");
    prefs.set("k", "v").unwrap();
    assert!(prefs.remove("k").unwrap());
    assert!(!prefs.remove("k").unwrap(), "second remove: absent");
    assert_eq!(prefs.get::<String>("k").unwrap(), None);
}

#[test]
fn clear_empties_and_persists() {
    let (dir, prefs) = tmp_prefs("clear");
    prefs.set("a", 1u32).unwrap();
    prefs.set("b", "x").unwrap();
    prefs.clear().unwrap();
    assert_eq!(prefs.entries(), vec![]);
    let reloaded = Prefs::load(&dir.join("system_prefs.json")).unwrap();
    assert_eq!(reloaded.entries(), vec![]);
}

#[test]
fn get_or_returns_stored_or_default() {
    let (_dir, prefs) = tmp_prefs("getor");
    prefs.set("http_port", 9000u64).unwrap();
    assert_eq!(prefs.get_or("http_port", 8080u64), 9000);
    assert_eq!(prefs.get_or("https_port", 8443u64), 8443);
    // Type mismatch falls back to the default instead of erroring out.
    prefs.set("flag", "not-a-bool").unwrap();
    assert!(!prefs.get_or("flag", false));
}

#[test]
fn entries_sorted_with_json_rendered_values() {
    let (_dir, prefs) = tmp_prefs("entries");
    prefs.set("z_key", "v").unwrap();
    prefs.set("a_key", vec![1u32, 2]).unwrap();

    let entries = prefs.entries_sorted();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].0, "a_key");
    assert_eq!(entries[0].1, "[1,2]");
    assert_eq!(entries[1].0, "z_key");
    // Strings render as JSON (quoted) — plain-desktop parity.
    assert_eq!(entries[1].1, "\"v\"");

    let values = prefs.entries();
    assert_eq!(values[0].1, serde_json::json!([1u32, 2]));
    assert_eq!(values[1].1, serde_json::json!("v"));
}

#[test]
fn reload_sees_previous_writes_and_pretty_prints() {
    let (dir, prefs) = tmp_prefs("reload");
    prefs.set("device_name", "box").unwrap();
    prefs.set("nested", serde_json::json!({"a": 1})).unwrap();
    drop(prefs);

    // File is pretty-printed (hand-editable, like plain-desktop's).
    let text = std::fs::read_to_string(dir.join("system_prefs.json")).unwrap();
    assert!(text.contains("\n  \"device_name\":"), "pretty: {text}");

    let reloaded = Prefs::load(&dir.join("system_prefs.json")).unwrap();
    assert_eq!(
        reloaded.get::<String>("device_name").unwrap().as_deref(),
        Some("box")
    );
    assert_eq!(
        reloaded.get::<serde_json::Value>("nested").unwrap(),
        Some(serde_json::json!({"a": 1}))
    );
}

/// Both preference files load independently and retain their own key spaces.
#[test]
fn loads_system_and_user_files_independently() {
    let (dir, _prefs) = tmp_prefs("split-reload");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("system_prefs.json"),
        r#"{"client_id":"ab12cd34","device_name":"MacBook-Pro","http_port":9000}"#,
    )
    .unwrap();

    std::fs::write(dir.join("user_prefs.json"), r#"{"theme":"dark"}"#).unwrap();

    let prefs = Prefs::load(&default_path(&dir)).unwrap();
    assert_eq!(
        prefs.get::<String>("client_id").unwrap().as_deref(),
        Some("ab12cd34")
    );
    assert_eq!(prefs.get_or("http_port", 8080u64), 9000);
    assert_eq!(
        prefs.get_user::<String>("theme").unwrap().as_deref(),
        Some("dark")
    );
    assert_eq!(prefs.get::<String>("theme").unwrap(), None);
}

#[test]
fn corrupt_file_fails_loudly() {
    let (dir, _prefs) = tmp_prefs("corrupt");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("system_prefs.json"), "{not json").unwrap();
    assert!(Prefs::load(&dir.join("system_prefs.json")).is_err());
}

#[test]
fn global_accessor_contract() {
    let (_dir, prefs) = tmp_prefs("global");
    let prefs = std::sync::Arc::new(prefs);
    prefs.set("device_name", "global-box").unwrap();
    set_global(prefs);
    assert_eq!(
        get_default()
            .get::<String>("device_name")
            .unwrap()
            .as_deref(),
        Some("global-box")
    );
    assert!(try_get_default().is_some());
}
