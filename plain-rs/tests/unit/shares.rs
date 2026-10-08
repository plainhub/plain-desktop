use super::*;
fn fixture() -> (tempfile::TempDir, Arc<Service>) {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(Db::open(&dir.path().join("db")).unwrap());
    let prefs =
        Arc::new(Prefs::load_pair(&dir.path().join("system"), &dir.path().join("user")).unwrap());
    prefs
        .set("master_secret", crate::utils::base64::base64_encode(&[7; 32]))
        .unwrap();
    prefs.set_user("service", true).unwrap();
    (dir, Service::new(db, prefs))
}
fn create(service: &Service, paths: Vec<PathBuf>) -> ShareRow {
    service
        .create(
            "share".into(),
            paths.iter().map(|p| p.to_str().unwrap().into()).collect(),
            crate::utils::base64::base64_encode(&[8; 32]),
            true,
            None,
        )
        .unwrap()
}
#[test]
fn roots_traversal_and_files() {
    let (dir, s) = fixture();
    let root = dir.path().join("photos");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("a.jpg"), b"synthetic").unwrap();
    let file = dir.path().join("single.txt");
    std::fs::write(&file, b"text").unwrap();
    let row = create(&s, vec![root.clone(), root.clone(), file.clone()]);
    assert_eq!(Service::roots(&row).unwrap().len(), 2);
    assert_eq!(
        s.resolve(&row.id, "photos/./a.jpg", true).unwrap(),
        Some(
            std::fs::canonicalize(root.join("a.jpg"))
                .unwrap()
                .to_str()
                .unwrap()
                .into()
        )
    );
    for p in [
        "photos/../single.txt",
        "photos/../../etc/passwd",
        "single.txt/a",
        "unknown/a",
    ] {
        assert!(s.resolve(&row.id, p, true).unwrap().is_none());
    }
    assert!(s.resolve(&row.id, "single.txt", true).unwrap().is_some());
}
#[test]
fn duplicate_names_are_rejected_without_persisting() {
    let (dir, s) = fixture();
    let a = dir.path().join("a/docs");
    let b = dir.path().join("b/docs");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    assert!(
        s.create(
            "x".into(),
            vec![a.to_str().unwrap().into(), b.to_str().unwrap().into()],
            crate::utils::base64::base64_encode(&[8; 32]),
            true,
            None
        )
        .is_err()
    );
    assert!(s.db.share_list().unwrap().is_empty());
}
#[test]
fn fresh_auth_revocation_expiry_and_disabled_service() {
    let (dir, s) = fixture();
    let a = dir.path().join("a");
    std::fs::create_dir(&a).unwrap();
    let row = create(&s, vec![a]);
    assert!(s.active(&row.id, true).unwrap().is_some());
    let token = s.token(&row.id).unwrap();
    assert_eq!(crate::utils::base64::base64_decode_checked(&token).unwrap().len(), 32);
    s.prefs.set_user("service", false).unwrap();
    assert!(s.active(&row.id, true).unwrap().is_none());
    s.prefs.set_user("service", true).unwrap();
    s.update(
        &row.id,
        "expired",
        Some("2000-01-01T00:00:00Z".into()),
        None,
    )
    .unwrap();
    assert!(s.active(&row.id, true).unwrap().is_none());
    s.update(&row.id, "active", None, None).unwrap();
    assert!(s.active(&row.id, true).unwrap().is_some());
    s.db.share_delete(&row.id).unwrap();
    assert!(s.active(&row.id, true).unwrap().is_none());
    assert!(s.update(&row.id, "revive", None, None).is_err());
}
#[test]
fn encrypted_file_ids_are_share_bound_and_roots_are_fresh() {
    let (dir, s) = fixture();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    std::fs::create_dir(&a).unwrap();
    std::fs::create_dir(&b).unwrap();
    std::fs::write(a.join("x"), b"x").unwrap();
    let row = create(&s, vec![a]);
    let encrypt = |id: &str| {
        crate::utils::base64::base64_encode(&
            crate::xchacha_encrypt_raw(
                &[8; 32],
                serde_json::json!({"sharedId":id,"virtualPath":"a/x"})
                    .to_string()
                    .as_bytes(),
            )
            .unwrap(),
        )
    };
    let encoded = encrypt(&row.id);
    assert!(s.resolve_file(&row.id, &encoded).unwrap().is_some());
    assert!(
        s.resolve_file(&row.id, &encrypt("another-share"))
            .unwrap()
            .is_none()
    );
    assert!(s.resolve_file(&row.id, "invalid").unwrap().is_none());
    s.update(&row.id, "new", None, Some(vec![b.to_str().unwrap().into()]))
        .unwrap();
    assert!(s.resolve_file(&row.id, &encoded).unwrap().is_none());
    let restarted = Service::new(
        Arc::new(Db::open(&dir.path().join("db")).unwrap()),
        s.prefs.clone(),
    );
    assert_eq!(
        restarted.db.share_get(&row.id).unwrap().unwrap().name,
        "new"
    );
}
#[cfg(unix)]
#[test]
fn symlink_escape_and_root_replacement_are_rejected() {
    use std::os::unix::fs::symlink;
    let (dir, s) = fixture();
    let root = dir.path().join("root");
    let outside = dir.path().join("outside");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("secret"), b"private").unwrap();
    symlink(&outside, root.join("escape")).unwrap();
    let row = create(&s, vec![root.clone()]);
    assert!(
        s.resolve(&row.id, "root/escape/secret", true)
            .unwrap()
            .is_none()
    );
    std::fs::rename(&root, dir.path().join("old")).unwrap();
    symlink(&outside, &root).unwrap();
    assert!(s.resolve(&row.id, "root/secret", true).unwrap().is_none());
}
#[cfg(unix)]
#[test]
fn browse_and_archive_exclude_escaping_symlinks() {
    use std::os::unix::fs::symlink;
    let (dir, s) = fixture();
    let root = dir.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let outside = dir.path().join("secret");
    std::fs::write(&outside, b"private").unwrap();
    std::fs::write(root.join("a"), b"allowed").unwrap();
    std::fs::create_dir(root.join("empty")).unwrap();
    symlink(outside, root.join("escape")).unwrap();
    let row = create(&s, vec![root]);
    let (_, entries) = s.browse(&row.id, "root").unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|e| e.virtual_path.as_str())
            .collect::<Vec<_>>(),
        vec!["root/a", "root/empty"]
    );
    let encrypted = crate::utils::base64::base64_encode(&
        crate::xchacha_encrypt_raw(
            &[8; 32],
            serde_json::json!({"sharedId":row.id,"virtualPath":"root"})
                .to_string()
                .as_bytes(),
        )
        .unwrap(),
    );
    let zip = s.archive(&row.id, &encrypted).unwrap();
    assert_eq!(
        zip.iter()
            .map(|e| e.virtual_path.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "empty/"]
    );
}
#[test]
fn token_matches_standard_hmac_vector() {
    let (dir, s) = fixture();
    let root = dir.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let mut row = create(&s, vec![root]);
    row.id = "Hi There".into();
    s.db.share_save(&row).unwrap();
    s.prefs
        .set("master_secret", crate::utils::base64::base64_encode(&[0x0b; 32]))
        .unwrap();
    let expected = "198a607eb44bfbc69903a0f1cf2bbdc5ba0aa3f3d9ae3c1c7a3b1696a0b68cf7";
    let actual = crate::utils::base64::base64_decode_checked(&s.token(&row.id).unwrap()).unwrap();
    assert_eq!(crate::hex::bytes_to_hex(&actual), expected);
}
