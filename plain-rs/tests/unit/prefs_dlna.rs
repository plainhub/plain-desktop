//! Unit tests for `src/prefs/dlna.rs` — compiled as the `tests` child
//! module via `#[cfg(test)] #[path]` there.
use super::*;

fn tmp_prefs(tag: &str) -> Prefs {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "plain-rs-prefs-dlna-{tag}-{}-{nanos}",
        std::process::id()
    ));
    Prefs::load(&dir.join("prefs.json")).unwrap()
}

#[test]
fn enabled_defaults_false_and_roundtrips() {
    let prefs = tmp_prefs("enabled");
    assert!(!enabled(&prefs));
    set_enabled(&prefs, true);
    assert!(enabled(&prefs));
    assert!(Prefs::load(prefs.path()).unwrap().get_or("dlna", false));
}

#[test]
fn add_sender_replaces_previous_entry_for_same_ip() {
    let prefs = tmp_prefs("add");
    add_sender(&prefs, "dlna_allowed_senders", "192.0.2.9", "TV");
    add_sender(&prefs, "dlna_allowed_senders", "192.0.2.9", "Living TV");
    add_sender(&prefs, "dlna_allowed_senders", "192.0.2.10", "Box");

    let list = senders(&prefs, "dlna_allowed_senders");
    assert_eq!(
        list,
        vec![
            "192.0.2.9|Living TV".to_string(),
            "192.0.2.10|Box".to_string()
        ]
    );
    assert!(senders_contain_ip(&list, "192.0.2.9"));
    assert!(!senders_contain_ip(&list, "192.0.2.99"));
}

#[test]
fn remove_sender_filters_by_ip_across_names() {
    let prefs = tmp_prefs("remove");
    add_sender(&prefs, "dlna_denied_senders", "198.51.100.1", "A");
    add_sender(&prefs, "dlna_denied_senders", "198.51.100.2", "B");
    remove_sender(&prefs, "dlna_denied_senders", "198.51.100.1");

    let list = senders(&prefs, "dlna_denied_senders");
    assert_eq!(list, vec!["198.51.100.2|B".to_string()]);
    // Removing an absent ip is a no-op.
    remove_sender(&prefs, "dlna_denied_senders", "198.51.100.99");
    assert_eq!(senders(&prefs, "dlna_denied_senders").len(), 1);
}

#[test]
fn lists_are_independent_per_key() {
    let prefs = tmp_prefs("keys");
    add_sender(&prefs, "dlna_allowed_senders", "203.0.113.1", "x");
    assert!(senders(&prefs, "dlna_denied_senders").is_empty());
}

#[test]
fn decode_sender_entry_without_separator_keeps_whole_as_ip() {
    assert_eq!(
        decode_sender_entry("192.0.2.9"),
        ("192.0.2.9".to_string(), String::new())
    );
    assert_eq!(
        decode_sender_entry("192.0.2.9|TV"),
        ("192.0.2.9".to_string(), "TV".to_string())
    );
}
