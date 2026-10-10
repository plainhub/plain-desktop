//! Unit tests for `src/db/url_token.rs` — compiled as the `tests` child
//! module via `#[cfg(test)] #[path]` there.
use super::*;

fn test_prefs() -> (tempfile::TempDir, Prefs) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Prefs::load(&dir.path().join("prefs.json")).unwrap();
    (dir, prefs)
}

#[test]
fn ensure_generates_valid_and_stable_token() {
    let (_dir, prefs) = test_prefs();
    assert_eq!(prefs.get::<String>(URL_TOKEN_KEY).unwrap(), None);

    let first = UrlToken::new(&prefs).ensure().unwrap();
    assert_eq!(first.len(), 44, "base64 of 32 random bytes");
    assert_eq!(UrlToken::new(&prefs).ensure().unwrap(), first);
    // Stored as the `url_token` preference (plain-app/desktop key name).
    assert_eq!(
        prefs.get::<String>(URL_TOKEN_KEY).unwrap().as_deref(),
        Some(first.as_str())
    );
}

#[test]
fn invalid_stored_token_is_regenerated() {
    let (_dir, prefs) = test_prefs();
    prefs
        .set(URL_TOKEN_KEY, "not-base64-of-32-bytes!!")
        .unwrap();
    let token = UrlToken::new(&prefs).ensure().unwrap();
    assert_eq!(token.len(), 44, "malformed entry replaced");
    // The malformed value is overwritten in storage.
    assert_eq!(
        prefs.get::<String>(URL_TOKEN_KEY).unwrap().as_deref(),
        Some(token.as_str())
    );
}
