//! First-run bootstrap of the persisted device identity and server
//! secrets — the read-or-generate-and-persist flows previously
//! duplicated in plain-nas (`db::server_client_id` sibling logic) and
//! the plain-desktop Tauri shells. Key names are the plain-app
//! DataStore contract: `client_id` / `device_name` /
//! `signature_key_pair` / `url_token` / `mdns_hostname`.

use super::Prefs;

use crate::{base64_encode, ed25519_generate, gen_token, short_uuid};

/// Persistent device identity loaded from the preferences store.
#[derive(Clone, Debug)]
pub struct AppIdentity {
    pub client_id: String,
    pub device_name: String,
    /// Base64-encoded Ed25519 keypair bytes (64 bytes: private || public).
    pub ed25519_keypair: String,
}

/// Default device name shown on first run (before the user renames):
/// the OS hostname (`.local`/trailing dot stripped), falling back to
/// `Plain-<short uuid>` when the hostname is empty.
pub fn default_device_name() -> String {
    let host = crate::hostname::get();
    let trimmed = host.trim().trim_end_matches('.').trim_end_matches(".local");
    if trimmed.is_empty() {
        format!("Plain-{}", short_uuid::short_uuid())
    } else {
        trimmed.to_string()
    }
}

/// Build a fresh device identity (first run). The host persists the three
/// fields under the keys `client_id` / `device_name` /
/// `signature_key_pair`.
pub fn generate_identity() -> AppIdentity {
    let (kp, _) = ed25519_generate();
    AppIdentity {
        client_id: short_uuid::short_uuid(),
        device_name: default_device_name(),
        ed25519_keypair: base64_encode(&kp),
    }
}

/// Stored string for `key`, persisting `make()` when absent — the
/// read-or-generate core shared by every ensure flow below.
fn ensure_string(prefs: &Prefs, key: &str, make: impl FnOnce() -> String) -> String {
    if let Some(v) = prefs.get::<String>(key).unwrap_or(None)
        && !v.is_empty()
    {
        return v;
    }
    let v = make();
    let _ = prefs.set(key, &v);
    v
}

/// Load (or generate on first run) the device identity from the
/// preferences store. Idempotent: a second call returns the persisted
/// values without rewriting the file. A stored `device_name` is
/// normalized (whitespace, trailing `.`/`.local` stripped) on read.
pub fn ensure_identity(prefs: &Prefs) -> AppIdentity {
    let client_id = ensure_string(prefs, "client_id", short_uuid::short_uuid);
    let device_name = match prefs.get::<String>("device_name").unwrap_or(None) {
        Some(v) => {
            let v = v
                .trim()
                .trim_end_matches('.')
                .trim_end_matches(".local")
                .to_string();
            if v.is_empty() {
                ensure_string(prefs, "device_name", default_device_name)
            } else {
                v
            }
        }
        None => ensure_string(prefs, "device_name", default_device_name),
    };
    let ed25519_keypair = ensure_string(prefs, "signature_key_pair", || {
        let (kp, _) = ed25519_generate();
        base64_encode(&kp)
    });
    AppIdentity {
        client_id,
        device_name,
        ed25519_keypair,
    }
}

/// Return the persistent local-server URL token, generating it on first
/// run. A token that is base64 of 32 zero bytes can only come from a
/// build where the RNG silently failed (plain-rs gen_random pre-fix on
/// Windows) — regenerate it instead of serving it forever.
pub fn ensure_url_token(prefs: &Prefs) -> String {
    let zero_token = base64_encode(&[0u8; 32]);
    match prefs.get::<String>("url_token").unwrap_or(None) {
        Some(t) if !t.is_empty() && t != zero_token => t,
        _ => {
            let token = gen_token();
            let _ = prefs.set("url_token", &token);
            token
        }
    }
}

/// Allowed chars for the random mDNS host label — mirrors plain-app's
/// `MdnsHostnamePreference`: `('a'..'z')` minus the ambiguous `i l o v`.
const MDNS_HOSTNAME_CHARS: &[u8] = b"abcdefghjkmnpqrstuwxyz";

fn random_mdns_hostname_label() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..2)
        .map(|_| MDNS_HOSTNAME_CHARS[rng.gen_range(0..MDNS_HOSTNAME_CHARS.len())] as char)
        .collect()
}

/// mDNS hostname for local-network discovery — mirrors plain-app's
/// `MdnsHostnamePreference.ensureValueAsync`: returns the stored value,
/// or on first run generates a random two-char host under `.local` and
/// persists it.
pub fn ensure_mdns_hostname(prefs: &Prefs) -> String {
    ensure_string(prefs, "mdns_hostname", || {
        format!("{}.local", random_mdns_hostname_label())
    })
}

#[cfg(test)]
#[path = "../../tests/unit/prefs_identity.rs"]
mod tests;
