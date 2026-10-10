//! Unit tests for `src/db/signature_key.rs` — compiled as the `tests`
//! child module via `#[cfg(test)] #[path]` there.
use super::*;

fn test_prefs(tag: &str) -> (tempfile::TempDir, Prefs) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Prefs::load(&dir.path().join(format!("{tag}-prefs.json"))).unwrap();
    (dir, prefs)
}

#[test]
fn ensure_is_stable_and_wellformed() {
    let (_dir, prefs) = test_prefs("stable");
    let first = SignatureKey::new(&prefs).ensure().unwrap();
    let second = SignatureKey::new(&prefs).ensure().unwrap();
    assert_eq!(first, second, "public key must be stable across calls");
    assert_eq!(
        plain_rs::utils::base64::base64_decode_checked(&first)
            .unwrap()
            .len(),
        32,
        "signaturePublicKey is base64 of a 32-byte ed25519 public key"
    );
}

#[test]
fn persisted_as_base64_keypair_like_plain_desktop() {
    let (dir, prefs) = test_prefs("b64");
    SignatureKey::new(&prefs).ensure().unwrap();

    // The preference is the base64 of the 64-byte keypair — readable on
    // all shells share the core storage shape.
    let stored = prefs
        .get::<String>(plain_rs::prefs::system::keys::SIGNATURE_KEY_PAIR)
        .unwrap()
        .expect("keypair pref written");
    let bytes = plain_rs::utils::base64::base64_decode_checked(&stored).unwrap();
    assert_eq!(bytes.len(), 64);

    // Reload from the same file: same identity, sign/verify roundtrip.
    let reloaded = Prefs::load(&dir.path().join("b64-prefs.json")).unwrap();
    assert_eq!(
        SignatureKey::new(&reloaded).ensure().unwrap(),
        SignatureKey::new(&prefs).ensure().unwrap()
    );
    let kp = SignatureKey::new(&reloaded).ensure_keypair().unwrap();
    let sig = plain_rs::crypto::ed25519_sign(&kp, b"msg");
    let pk = SignatureKey::new(&reloaded).ensure().unwrap();
    assert!(plain_rs::crypto::ed25519_verify(&pk, b"msg", &sig));
}

#[test]
fn corrupted_identity_is_refused_without_replacement() {
    let (_dir, prefs) = test_prefs("corrupt");
    SignatureKey::new(&prefs).ensure().unwrap();
    prefs
        .set(plain_rs::prefs::system::keys::SIGNATURE_KEY_PAIR, "short")
        .unwrap();
    assert!(SignatureKey::new(&prefs).ensure().is_err());
    assert_eq!(
        prefs
            .get::<String>(plain_rs::prefs::system::keys::SIGNATURE_KEY_PAIR)
            .unwrap()
            .as_deref(),
        Some("short")
    );
}
