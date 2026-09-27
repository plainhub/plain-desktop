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

    let CertifiedKey { cert, key_pair } =
        generate_simple_self_signed(san_names.to_vec()).map_err(io::Error::other)?;

    let cert_pem = cert.pem().into_bytes();
    let key_pem = key_pair.serialize_pem().into_bytes();

    std::fs::write(cert_path, &cert_pem)?;
    std::fs::write(key_path, &key_pem)?;
    log::info!("tls: certificate written to {}", cert_path.display());

    Ok((cert_pem, key_pem))
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
}
