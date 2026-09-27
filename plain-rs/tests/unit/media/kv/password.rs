//! Unit tests for `src/db/password.rs` — compiled as the `tests` child
//! module via `#[cfg(test)] #[path]` there.
use super::*;

fn test_prefs() -> (tempfile::TempDir, Prefs) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Prefs::load(&dir.path().join("prefs.json")).unwrap();
    (dir, prefs)
}

fn hash_hex(seed: u8) -> String {
    vec![seed; 128]
        .into_iter()
        .map(|b| format!("{b:x}"))
        .collect::<String>()
        .chars()
        .take(128)
        .collect()
}

#[test]
fn roundtrip_and_validation() {
    let (_dir, prefs) = test_prefs();
    assert!(!PasswordStore::new(&prefs).has());

    let hash = "a".repeat(128);
    PasswordStore::new(&prefs).set(&hash).unwrap();
    assert!(PasswordStore::new(&prefs).has());
    assert_eq!(
        PasswordStore::new(&prefs).get().as_deref(),
        Some(hash.as_str())
    );

    // Malformed hashes are rejected before any write.
    assert!(PasswordStore::new(&prefs).set("short").is_err());
    assert!(PasswordStore::new(&prefs).set(&"z".repeat(128)).is_err());
    assert_eq!(
        PasswordStore::new(&prefs).get().as_deref(),
        Some(hash.as_str()),
        "rejected writes must not clobber the stored hash"
    );
}

#[test]
fn stored_as_password_hash_preference() {
    let (dir, prefs) = test_prefs();
    let hash = hash_hex(b'a');
    PasswordStore::new(&prefs).set(&hash).unwrap();

    // Visible on the DataStore page as a plain preference.
    let reloaded = Prefs::load(&dir.path().join("prefs.json")).unwrap();
    assert_eq!(
        reloaded.get::<String>("password_hash").unwrap().as_deref(),
        Some(hash.as_str())
    );
}
