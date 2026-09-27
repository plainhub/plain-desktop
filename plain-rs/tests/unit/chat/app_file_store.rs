use super::*;

fn make_weak_hash_for(data: &[u8]) -> (String, i64) {
    let mut hasher = Sha256::new();
    if data.len() <= WEAK_HEAD + WEAK_TAIL {
        hasher.update(data);
    } else {
        hasher.update(&data[..WEAK_HEAD]);
        hasher.update(&data[data.len() - WEAK_TAIL..]);
    }
    (bytes_to_hex(&hasher.finalize()), data.len() as i64)
}

fn unique_tmp_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "plain-rs-chat-appfile-{label}-{}-{nanos}",
        std::process::id()
    ))
}

fn write_src(dir: &Path, name: &str, data: &[u8]) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, data).unwrap();
    p
}

#[test]
fn fid_ext_maps_unknown_to_empty() {
    assert_eq!(fid_ext("image/jpeg"), "jpg");
    assert_eq!(fid_ext("image/png"), "png");
    assert_eq!(fid_ext("image/x-icon"), "ico"); // favicon content type
    assert_eq!(fid_ext("image/vnd.microsoft.icon"), "ico");
    assert_eq!(fid_ext("video/mp4"), "mp4");
    assert_eq!(fid_ext("application/pdf"), "pdf");
    assert_eq!(fid_ext("text/markdown"), "md");
    assert_eq!(fid_ext(""), "");
    assert_eq!(fid_ext("application/octet-stream"), "");
    assert_eq!(fid_ext("IMAGE/PNG"), "png"); // case-insensitive
}

#[test]
fn ext_from_name_prefers_filename_extension() {
    assert_eq!(ext_from_name("local.properties", ""), "properties");
    assert_eq!(
        ext_from_name("local.properties", "application/octet-stream"),
        "properties"
    );
    assert_eq!(ext_from_name("Photo.JPG", ""), "jpg"); // lowercased
    assert_eq!(ext_from_name("archive.tar.gz", ""), "gz");
    // No extension in the name — falls back to the MIME table.
    assert_eq!(ext_from_name("README", "image/png"), "png");
    assert_eq!(ext_from_name("README", "text/plain"), "txt");
    // No extension and unknown MIME — no extension on disk.
    assert_eq!(ext_from_name("README", "application/octet-stream"), "");
    assert_eq!(ext_from_name("README", ""), "");
    // A dotfile like `.gitignore` has no extension per Path semantics.
    assert_eq!(ext_from_name(".gitignore", "text/plain"), "txt");
}

#[test]
fn import_file_keeps_extension_from_filename() {
    let dir = unique_tmp_dir("import-ext");
    let db = ChatDb::open(&dir.join("local_chat.db")).unwrap();
    let src = write_src(&dir, "src.bin", b"sdk.dir=/opt/android/sdk\n");

    // Browsers send no Content-Type for `.properties` files.
    let result = import_file(&db, &dir, &src, "local.properties", "").unwrap();
    assert_eq!(result.mime_type, DEFAULT_MIME);
    assert!(
        result.fid_suffix.ends_with(".properties"),
        "{}",
        result.fid_suffix
    );
    assert!(result.real_path.exists());
    assert_eq!(result.real_path, dest_path(&dir, &result.id, "properties"));
    assert!(!result.reused);

    // Re-importing the same content (dedup) must return the SAME
    // suffix and path — the reuse branch must not recompute the
    // path from the MIME and point at an extension-less file.
    let again = import_file(&db, &dir, &src, "local.properties", "").unwrap();
    assert!(again.reused);
    assert_eq!(again.fid_suffix, result.fid_suffix);
    assert_eq!(again.real_path, result.real_path);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn import_file_falls_back_to_mime_when_name_has_no_ext() {
    let dir = unique_tmp_dir("import-mime-ext");
    let db = ChatDb::open(&dir.join("local_chat.db")).unwrap();
    let src = write_src(&dir, "src.bin", b"\x89PNGfakepng");

    let result = import_file(&db, &dir, &src, "photo", "image/png").unwrap();
    assert!(result.fid_suffix.ends_with(".png"), "{}", result.fid_suffix);
    assert_eq!(result.real_path, dest_path(&dir, &result.id, "png"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn import_bytes_dedupes_on_second_call() {
    let dir = unique_tmp_dir("import-bytes");
    let db = ChatDb::open(&dir.join("local_chat.db")).unwrap();

    let first = import_bytes(&db, &dir, b"hello attachment", "text/plain").unwrap();
    assert!(!first.reused);
    assert!(first.fid_suffix.ends_with(".txt"));

    let second = import_bytes(&db, &dir, b"hello attachment", "text/plain").unwrap();
    assert!(second.reused);
    assert_eq!(second.fid_suffix, first.fid_suffix);
    assert_eq!(db.get_app_file(&first.id).unwrap().ref_count, 2);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn strong_hash_is_deterministic() {
    let h1 = {
        let mut h = Sha256::new();
        h.update(b"hello world");
        bytes_to_hex(&h.finalize())
    };
    let h2 = {
        let mut h = Sha256::new();
        h.update(b"hello world");
        bytes_to_hex(&h.finalize())
    };
    assert_eq!(h1, h2);
    assert_eq!(h1.len(), 64);
}

#[test]
fn weak_hash_collapses_head_tail_for_small_file() {
    // For files smaller than HEAD + TAIL, the whole file is hashed.
    let data = b"small content";
    let (h, size) = make_weak_hash_for(data);
    assert_eq!(size, data.len() as i64);
    let mut hasher = Sha256::new();
    hasher.update(data);
    assert_eq!(h, bytes_to_hex(&hasher.finalize()));
}

#[test]
fn weak_hash_uses_head_and_tail_for_large_file() {
    // Build a 16 KB blob — large enough that the head/tail windows are
    // separate (4 KB + 4 KB, with 8 KB in the middle that must NOT be
    // hashed).
    let mut data = vec![0u8; 16 * 1024];
    for (i, b) in data.iter_mut().enumerate() {
        *b = (i & 0xff) as u8;
    }
    // Middle 8 KB — set to 0xFF so any failure to skip it would change
    // the hash.
    for b in &mut data[4 * 1024..12 * 1024] {
        *b = 0xff;
    }
    let (h, size) = make_weak_hash_for(&data);
    assert_eq!(size, data.len() as i64);

    let mut hasher = Sha256::new();
    hasher.update(&data[..4 * 1024]);
    hasher.update(&data[12 * 1024..]);
    let expected = bytes_to_hex(&hasher.finalize());
    assert_eq!(h, expected);

    // Sanity: hashing the whole file gives a different value.
    let mut full = Sha256::new();
    full.update(&data);
    assert_ne!(h, bytes_to_hex(&full.finalize()));
}

#[test]
fn relative_dest_path_sharded() {
    assert_eq!(
        relative_dest_path("abcdef0123456789", "jpg"),
        "files/ab/cd/abcdef0123456789.jpg"
    );
}

#[test]
fn relative_dest_path_without_ext() {
    assert_eq!(
        relative_dest_path("abcdef0123456789", ""),
        "files/ab/cd/abcdef0123456789"
    );
}

#[test]
fn relative_dest_path_short_hash_falls_back_flat() {
    assert_eq!(relative_dest_path("ab", "jpg"), "files/ab.jpg");
    assert_eq!(relative_dest_path("ab", ""), "files/ab");
}

#[test]
fn dest_path_joins_data_dir_with_relative() {
    let dir = std::path::Path::new("/data");
    assert_eq!(
        dest_path(dir, "abcdef0123456789", "jpg"),
        std::path::PathBuf::from("/data/files/ab/cd/abcdef0123456789.jpg")
    );
}

fn seed_named_chat(db: &ChatDb, at: &str, uri: &str, name: &str) {
    let mut chat = crate::chat::db::DChat::new(
        "me",
        "peer1",
        "",
        &serde_json::json!({
            "type": "FILES",
            "value": { "items": [ {"uri": uri, "fileName": name} ] }
        })
        .to_string(),
    );
    chat.id = format!("c-{at}");
    chat.created_at = at.to_string();
    chat.updated_at = at.to_string();
    db.insert_chat(&chat);
}

#[test]
fn file_name_map_prefers_newest_chat_and_strips_fid_ext() {
    let dir = unique_tmp_dir("name-map");
    let db = ChatDb::open(&dir.join("local_chat.db")).unwrap();
    seed_named_chat(&db, "2026-01-01T00:00:01Z", "fid:aa11.jpg", "old.jpg");
    seed_named_chat(&db, "2026-01-02T00:00:01Z", "fid:aa11.jpg", "new.jpg");
    // key is the bare hash — extension stripped.
    let map = file_name_map(&db.get_all_chats());
    assert_eq!(map.get("aa11").map(String::as_str), Some("new.jpg"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn display_name_falls_back_to_mime_extension() {
    let mut f = DAppFile {
        id: "bb22".to_string(),
        size: 1,
        mime_type: "image/png".to_string(),
        real_path: String::new(),
        ref_count: 1,
        weak_hash: String::new(),
        created_at: String::new(),
        updated_at: String::new(),
    };
    let mut map = std::collections::HashMap::new();
    map.insert("bb22".to_string(), "  ".to_string()); // blank name ignored
    assert_eq!(display_name(&f, &map), "file.png");
    f.mime_type = "application/octet-stream".to_string();
    assert_eq!(display_name(&f, &map), "file");
}
