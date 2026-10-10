//! Unit tests for `src/prefs/identity.rs` — compiled as the `tests`
//! child module via `#[cfg(test)] #[path]` there.
use super::*;

fn tmp_prefs(tag: &str) -> Prefs {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "plain-rs-prefs-id-{tag}-{}-{nanos}",
        std::process::id()
    ));
    Prefs::load(&dir.join("prefs.json")).unwrap()
}

#[test]
fn ensure_identity_generates_and_persists_all_three_keys() {
    let prefs = tmp_prefs("identity");
    let id = ensure_identity(&prefs);
    assert!(!id.client_id.is_empty());
    assert!(!id.device_name.is_empty());
    assert!(!id.ed25519_keypair.is_empty());

    // All three keys persisted under the plain-app contract names.
    assert_eq!(
        prefs.get::<String>("client_id").unwrap().as_deref(),
        Some(id.client_id.as_str())
    );
    assert_eq!(
        prefs.get_user::<String>("device_name").unwrap().as_deref(),
        Some(id.device_name.as_str())
    );
    assert_eq!(
        prefs
            .get::<String>("signature_key_pair")
            .unwrap()
            .as_deref(),
        Some(id.ed25519_keypair.as_str())
    );
}

#[test]
fn ensure_identity_is_idempotent_and_reuses_stored_values() {
    let prefs = tmp_prefs("idempotent");
    let first = ensure_identity(&prefs);
    let second = ensure_identity(&prefs);
    assert_eq!(first.client_id, second.client_id);
    assert_eq!(first.device_name, second.device_name);
    assert_eq!(first.ed25519_keypair, second.ed25519_keypair);

    // Pre-stored values win over generation.
    prefs.set("client_id", "existing").unwrap();
    assert_eq!(ensure_identity(&prefs).client_id, "existing");
}

#[test]
fn ensure_identity_normalizes_stored_device_name() {
    let prefs = tmp_prefs("normalize");
    prefs.set_user("device_name", "Box.local").unwrap();
    assert_eq!(ensure_identity(&prefs).device_name, "Box");
    prefs.set_user("device_name", "  ").unwrap();
    let regenerated = ensure_identity(&prefs).device_name;
    assert!(!regenerated.is_empty(), "blank name regenerates");
}

#[test]
fn ensure_url_token_generates_persists_and_is_stable() {
    let prefs = tmp_prefs("urltoken");
    let t1 = ensure_url_token(&prefs);
    assert!(!t1.is_empty());
    assert_eq!(ensure_url_token(&prefs), t1, "stored token reused");

    // Stored token survives reload.
    let reloaded = Prefs::load(prefs.path()).unwrap();
    assert_eq!(ensure_url_token(&reloaded), t1);
}

#[test]
fn ensure_url_token_regenerates_zero_token() {
    let prefs = tmp_prefs("zerotoken");
    let zero = base64_encode(&[0u8; 32]);
    prefs.set("url_token", zero.as_str()).unwrap();
    let t = ensure_url_token(&prefs);
    assert_ne!(t, zero, "all-zero token is regenerated");
}

#[test]
fn ensure_mdns_hostname_uses_the_shared_default() {
    let prefs = tmp_prefs("mdns");
    let h = ensure_mdns_hostname(&prefs);
    assert_eq!(h, "plainapp.local");
    // Persisted + stable across reloads.
    assert_eq!(
        prefs.get::<String>("mdns_hostname").unwrap().as_deref(),
        Some(h.as_str())
    );
    let reloaded = Prefs::load(prefs.path()).unwrap();
    assert_eq!(ensure_mdns_hostname(&reloaded), h);
}

#[test]
fn default_device_name_prefers_hostname() {
    // Deterministic on the host running the test: either the trimmed
    // OS hostname or the Plain-<shortid> fallback — never empty, never
    // carrying a `.local` suffix.
    let name = default_device_name();
    assert!(!name.is_empty());
    assert!(!name.ends_with(".local"));
    assert!(!name.ends_with('.'));
}
