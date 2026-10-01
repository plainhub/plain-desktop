use super::*;

#[test]
fn preference_key_rejects_empty_or_unsafe_keys() {
    assert!(validate_pref_key("homeFeatures").is_ok());
    let long_key = "x".repeat(129);
    for key in ["", "a/b", "a b", "admin:password", "é", long_key.as_str()] {
        assert!(validate_pref_key(key).is_err(), "accepted {key:?}");
    }
}

#[test]
fn user_preference_values_keep_native_json_types() {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Prefs::load_pair(
        &dir.path().join("system_prefs.json"),
        &dir.path().join("user_prefs.json"),
    )
    .unwrap();
    prefs.set("theme", "dark").unwrap();
    prefs
        .set_user("theme", serde_json::json!({"dark": true, "scale": [1, 2]}))
        .unwrap();

    assert_eq!(prefs.get::<String>("theme").unwrap().as_deref(), Some("dark"));
    assert_eq!(
        prefs.get_user::<serde_json::Value>("theme").unwrap(),
        Some(serde_json::json!({"dark": true, "scale": [1, 2]})),
    );
}
