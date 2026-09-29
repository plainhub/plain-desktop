use super::*;

#[test]
fn prefs_only_exposes_admin_string_values_without_the_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Prefs::load(&dir.path().join("prefs.json")).unwrap();
    prefs.set("password", "secret").unwrap();
    prefs.set("other.theme", "dark").unwrap();
    prefs.set("admin.theme", "light").unwrap();
    prefs.set("admin.count", 3).unwrap();

    let entries = list_prefs(&prefs);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].key, "theme");
    assert_eq!(entries[0].value, "light");
}

#[test]
fn pref_key_rejects_empty_or_unsafe_keys() {
    assert_eq!(pref_key("homeFeatures").unwrap(), "admin.homeFeatures");
    let long_key = "x".repeat(129);
    for key in ["", "a/b", "a b", "admin:password", "é", long_key.as_str()] {
        assert!(pref_key(key).is_err(), "accepted {key:?}");
    }
}
