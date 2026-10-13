use super::*;
#[test]
fn persists_rotates_and_rejects_invalid_identity() {
    let dir = std::env::temp_dir().join(format!("plain-mobile-tls-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("identity.json");
    let first = load(&path).unwrap();
    assert_eq!(first, load(&path).unwrap());
    assert!(!signature(&first.0).unwrap().is_empty());
    let second = generate(&path).unwrap();
    assert_ne!(signature(&first.0).unwrap(), signature(&second.0).unwrap());
    assert!(save(&path, &first.0, &second.1).is_err());
    assert_eq!(second, load(&path).unwrap());
    assert!(signature(b"invalid PEM").is_err());
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn rejects_truncated_and_overflowing_der() {
    for bytes in [
        &[0x30, 0x80][..],
        &[0x30, 0x82, 0xff][..],
        &[0x30, 0x01][..],
        &[0x30, 0x89, 0, 0, 0, 0, 0, 0, 0, 0, 0][..],
    ] {
        let mut input = bytes;
        assert!(tlv(&mut input, 0x30).is_err());
    }
}

#[test]
fn configured_pem_identity_is_shared_by_restart_and_certificate_rotation() {
    let directory = tempfile::tempdir().unwrap();
    let certificate = directory.path().join("cert.pem");
    let key = directory.path().join("key.pem");
    let source = IdentitySource::Pem {
        certificate: certificate.clone(),
        key: key.clone(),
    };
    let first = source.identity().unwrap();
    assert_eq!(source.identity().unwrap(), first);
    source.action(r#"{"action":"generate"}"#).unwrap();
    let rotated = source.identity().unwrap();
    assert_ne!(rotated.0, first.0);
    assert_eq!(rotated.0, std::fs::read(certificate).unwrap());
    assert_eq!(rotated.1, std::fs::read(key).unwrap());
    assert!(!directory.path().join("tls-identity.json").exists());
}
