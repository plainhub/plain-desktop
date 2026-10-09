//! Self-signed TLS certificate management for the local HTTPS servers.
//!
//! Shared by plain-desktop (`local/tls.rs`) and plain-nas (`cmd/run.rs`):
//! load `cert.pem` / `key.pem` from disk if both exist, otherwise generate
//! a self-signed certificate via `rcgen` and persist it. PEM bytes are
//! returned so each project can build its own TLS acceptor (desktop:
//! tokio-rustls, nas: axum-server / rustls) — the acceptor wiring is
//! deliberately NOT shared.

use std::io;
use std::path::Path;

use rcgen::{CertifiedKey, generate_simple_self_signed};

/// Ensure a self-signed certificate + key exist at `cert_path` / `key_path`.
/// If both files exist they are returned as-is; otherwise a new self-signed
/// certificate (EC P-256, per rcgen default) with the given subject alt
/// names is generated and written. Returns `(cert_pem, key_pem)` bytes.
///
/// `san_names` entries may be DNS names (`"localhost"`) or IP addresses
/// (`"127.0.0.1"`) — rcgen detects IPs automatically.
pub fn ensure_self_signed_pem(
    cert_path: &Path,
    key_path: &Path,
    san_names: &[String],
) -> io::Result<(Vec<u8>, Vec<u8>)> {
    if cert_path.exists() && key_path.exists() {
        let cert_pem = std::fs::read(cert_path)?;
        let key_pem = std::fs::read(key_path)?;
        log::info!("tls: loaded existing cert from {}", cert_path.display());
        return Ok((cert_pem, key_pem));
    }

    log::info!(
        "tls: generating new self-signed certificate in {}",
        cert_path.display()
    );
    // Create the parent dir when the cert path carries one (tests use
    // bare file names in flat temp dirs).
    let dir = cert_path.parent().filter(|d| !d.as_os_str().is_empty());
    if let Some(dir) = dir {
        std::fs::create_dir_all(dir)?;
    }

    let (cert_pem, key_pem) = generate_pem(san_names)?;

    std::fs::write(cert_path, &cert_pem)?;
    std::fs::write(key_path, &key_pem)?;
    log::info!("tls: certificate written to {}", cert_path.display());

    Ok((cert_pem, key_pem))
}

pub fn generate_pem(san_names: &[String]) -> io::Result<(Vec<u8>, Vec<u8>)> {
    let CertifiedKey { cert, key_pair } =
        generate_simple_self_signed(san_names.to_vec()).map_err(io::Error::other)?;
    Ok((
        cert.pem().into_bytes(),
        key_pair.serialize_pem().into_bytes(),
    ))
}

pub fn decode_pkcs12(bytes: &[u8], password: &str) -> Result<(Vec<u8>, Vec<u8>), String> {
    let store = p12_keystore::KeyStore::from_pkcs12(
        bytes,
        password,
        p12_keystore::Pkcs12ImportPolicy::Strict,
    )
    .map_err(|error| error.to_string())?;
    let (_, chain) = store
        .private_key_chain()
        .ok_or("No private key found in certificate file")?;
    if chain.certs().is_empty() {
        return Err("No certificate chain found in certificate file".into());
    }
    let pem = |label: &str, bytes: &[u8]| {
        let encoded = crate::utils::base64::base64_encode(bytes);
        let mut text = format!("-----BEGIN {label}-----\n");
        for line in encoded.as_bytes().chunks(64) {
            text.push_str(std::str::from_utf8(line).unwrap());
            text.push('\n');
        }
        text.push_str(&format!("-----END {label}-----\n"));
        text
    };
    let certificates = chain
        .certs()
        .iter()
        .map(|cert| pem("CERTIFICATE", cert.as_der()))
        .collect::<String>();
    Ok((
        certificates.into_bytes(),
        pem("PRIVATE KEY", chain.key().as_der()).into_bytes(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "plain-rs-tls-{tag}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn generates_then_reloads() {
        let dir = tmp_dir("gen");
        let cert = dir.join("cert.pem");
        let key = dir.join("key.pem");
        let sans = vec!["localhost".to_string(), "127.0.0.1".to_string()];

        let (c1, k1) = ensure_self_signed_pem(&cert, &key, &sans).unwrap();
        assert!(c1.starts_with(b"-----BEGIN CERTIFICATE-----"));
        assert!(k1.starts_with(b"-----BEGIN PRIVATE KEY-----"));
        assert!(cert.exists() && key.exists());

        // Second call must load from disk, not regenerate.
        let (c2, k2) = ensure_self_signed_pem(&cert, &key, &sans).unwrap();
        assert_eq!(c1, c2);
        assert_eq!(k1, k2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn regenerates_when_one_file_missing() {
        let dir = tmp_dir("partial");
        let cert = dir.join("cert.pem");
        let key = dir.join("key.pem");
        let sans = vec!["plainnas.local".to_string()];
        let (c1, _) = ensure_self_signed_pem(&cert, &key, &sans).unwrap();
        std::fs::remove_file(&key).unwrap();
        let (c2, _) = ensure_self_signed_pem(&cert, &key, &sans).unwrap();
        // Fresh pair — cert differs from the previous generation.
        assert_ne!(c1, c2);
        assert!(key.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn pkcs12_roundtrip_and_wrong_password_are_handled_in_rust() {
        use p12_keystore::{KeyStore, KeyStoreEntry, PrivateKeyChain, PrivateKey, Certificate};
        let cert=generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let key=PrivateKey::from_der(&cert.key_pair.serialize_der()).unwrap();
        let certificate=Certificate::from_der(cert.cert.der()).unwrap();
        let mut store=KeyStore::new();
        store.add_entry("server",KeyStoreEntry::PrivateKeyChain(PrivateKeyChain::new(vec![1u8],key,vec![certificate])));
        let bytes=store.writer("password").write().unwrap();
        let (chain,private)=decode_pkcs12(&bytes,"password").unwrap();
        assert_eq!(chain,cert.cert.pem().into_bytes());
        assert_eq!(private,cert.key_pair.serialize_pem().into_bytes());
        assert!(decode_pkcs12(&bytes,"wrong").is_err());
        assert!(decode_pkcs12(b"invalid","password").is_err());
        let empty=KeyStore::new().writer("").write().unwrap();
        assert!(decode_pkcs12(&empty,"").is_err());
    }

}
