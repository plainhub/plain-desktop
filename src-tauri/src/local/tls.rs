// TLS certificate management for the local HTTPS server.
// Certificate generation/persistence is shared: `plain_rs::tls`. This file
// only wires the generated PEM into a tokio-rustls acceptor.

use rustls_pemfile::{certs, private_key};
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;

const CERT_FILE: &str = "local_server_cert.pem";
const KEY_FILE: &str = "local_server_key.pem";

/// Ensure a self-signed certificate exists at `dir`. Generate one if not found.
/// Returns `(cert_pem_bytes, key_pem_bytes)`.
pub fn ensure_cert(dir: &Path) -> std::io::Result<(Vec<u8>, Vec<u8>)> {
    let cert_path = dir.join(CERT_FILE);
    let key_path = dir.join(KEY_FILE);
    let subject_alt_names = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    plain_rs::tls::ensure_self_signed_pem(&cert_path, &key_path, &subject_alt_names)
}

/// Build a `TlsAcceptor` from PEM-encoded cert + key bytes.
pub fn build_acceptor(cert_pem: &[u8], key_pem: &[u8]) -> std::io::Result<TlsAcceptor> {
    let certs: Vec<CertificateDer<'static>> = {
        let mut r = BufReader::new(cert_pem);
        certs(&mut r)
            .collect::<Result<Vec<_>, _>>()
            .map_err(std::io::Error::other)?
    };

    let key: PrivateKeyDer<'static> = {
        let mut r = BufReader::new(key_pem);
        private_key(&mut r)
            .map_err(std::io::Error::other)?
            .ok_or_else(|| std::io::Error::other("no private key found"))?
    };

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(std::io::Error::other)?;

    Ok(TlsAcceptor::from(Arc::new(config)))
}
