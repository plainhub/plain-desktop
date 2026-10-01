// TLS certificate management for the local HTTPS server.
// Certificate generation/persistence is shared: `crate::tls`. The host
// wires the generated PEM into its HTTPS listener (axum-server's
// `RustlsConfig::from_pem`).

use std::path::Path;

const CERT_FILE: &str = "local_server_cert.pem";
const KEY_FILE: &str = "local_server_key.pem";

/// Ensure a self-signed certificate exists at `dir`. Generate one if not found.
/// Returns `(cert_pem_bytes, key_pem_bytes)`.
pub fn ensure_cert(dir: &Path) -> std::io::Result<(Vec<u8>, Vec<u8>)> {
    let cert_path = dir.join(CERT_FILE);
    let key_path = dir.join(KEY_FILE);
    let subject_alt_names = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    crate::tls::ensure_self_signed_pem(&cert_path, &key_path, &subject_alt_names)
}
