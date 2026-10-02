use super::*;
async fn call(port: u16, token: &str, query: &str) -> serde_json::Value {
    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/graphql"))
        .bearer_auth(token)
        .header("content-type", "application/json")
        .body(serde_json::json!({"query":query}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    serde_json::from_str(&response.text().await.unwrap()).unwrap()
}
#[tokio::test]
async fn loopback_auth_note_lifecycle_persistence_and_bulk_guard() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plain.db");
    let token = crate::base64_encode(&[7; 32]);
    let server = ContentServer::start(
        &path,
        &token,
        Arc::new(crate::prefs::Prefs::load(&dir.path().join("system_prefs.json")).unwrap()),
    )
    .unwrap();
    let client = reqwest::Client::new();
    for bad in ["", "invalid"] {
        assert_eq!(
            client
                .get(format!("http://127.0.0.1:{}/health", server.port))
                .bearer_auth(bad)
                .send()
                .await
                .unwrap()
                .status(),
            401
        );
    }
    let result = call(
        server.port,
        &token,
        "mutation {createNote(input:{title:\"Title\",content:\"body\"}) {id}}",
    )
    .await;
    assert!(result.get("errors").is_none(), "{result}");
    let id = result["data"]["createNote"]["id"].as_str().unwrap();
    let result = call(
        server.port,
        &token,
        r#"mutation {trashNotes(query: " ") {affectedCount}}"#,
    )
    .await;
    assert!(result.get("errors").is_some());
    let result = call(
        server.port,
        &token,
        "mutation {trashNotes(query:\"all:true\") {affectedCount}}",
    )
    .await;
    assert_eq!(result["data"]["trashNotes"]["affectedCount"], 1);
    let result = call(
        server.port,
        &token,
        &format!("mutation {{restoreNotes(query:\"ids:{id}\") {{affectedCount}}}}"),
    )
    .await;
    assert_eq!(result["data"]["restoreNotes"]["affectedCount"], 1);
    server.shutdown().await;
    let second = ContentServer::start(
        &path,
        &crate::base64_encode(&[8; 32]),
        Arc::new(crate::prefs::Prefs::load(&dir.path().join("system_prefs.json")).unwrap()),
    )
    .unwrap();
    assert_eq!(Db::open(&path).unwrap().notes_count("").unwrap(), 1);
    assert_eq!(
        client
            .get(format!("http://127.0.0.1:{}/health", second.port))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    second.shutdown().await;
}
#[tokio::test]
async fn feed_dedup_read_state_and_note_link() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plain.db");
    let db = Db::open(&path).unwrap();
    let at = "2026-10-02T00:00:00.000Z";
    db.feed_save("feed", "News", "https://example.org/rss", false, at)
        .unwrap();
    let row = crate::db::notes_feeds::FeedEntryRow {
        id: "one".into(),
        feed_id: "feed".into(),
        title: "Article".into(),
        url: "https://example.org/one".into(),
        image: String::new(),
        description: "summary".into(),
        author: String::new(),
        content: String::new(),
        raw_id: "guid".into(),
        published_at: at.into(),
        read: false,
        created_at: at.into(),
        updated_at: at.into(),
    };
    db.feed_entries_insert(&[row.clone()]).unwrap();
    let mut duplicate = row;
    duplicate.id = "two".into();
    assert!(db.feed_entries_insert(&[duplicate]).unwrap().is_empty());
    let token = crate::base64_encode(&[9; 32]);
    let server = ContentServer::start(
        &path,
        &token,
        Arc::new(crate::prefs::Prefs::load(&dir.path().join("system_prefs.json")).unwrap()),
    )
    .unwrap();
    let result=call(server.port,&token,"mutation {markFeedEntriesRead(query:\"ids:one\",read:true) {affectedCount} saveFeedEntriesToNotes(query:\"ids:one\")}").await;
    assert!(result.get("errors").is_none(), "{result}");
    assert_eq!(result["data"]["markFeedEntriesRead"]["affectedCount"], 1);
    let entry = db.feed_entry_get("one").unwrap().unwrap();
    assert!(entry.read);
    assert_eq!(entry.updated_at, at);
    assert_eq!(
        db.note_get("one").unwrap().unwrap().content,
        "# Article\n\nsummary"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn cached_files_require_auth_and_keep_shared_references() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plain.db");
    let db = Arc::new(Db::open(&path).unwrap());
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap());
    let imported =
        crate::chat::app_file_store::import_bytes(&db, dir.path(), b"image fixture", "image/png")
            .unwrap();
    let second =
        crate::chat::app_file_store::import_bytes(&db, dir.path(), b"image fixture", "image/png")
            .unwrap();
    let uri = imported.real_path.to_string_lossy().to_string();
    let id = crate::base64_encode(
        &crate::xchacha_encrypt(&crate::prefs::ensure_url_token(&prefs), uri.as_bytes()).unwrap(),
    );
    let token = crate::base64_encode(&[10; 32]);
    let server = ContentServer::start(&path, &token, prefs).unwrap();
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/fs", server.port);
    assert_eq!(
        client
            .get(&url)
            .query(&[("id", &id)])
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let response = client
        .get(&url)
        .query(&[("id", &id)])
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.bytes().await.unwrap().as_ref(), b"image fixture");
    let assets = crate::feeds::FeedAssets {
        db: db.clone(),
        directory: dir.path().into(),
    };
    assets.release("/tmp/unowned-image.png");
    assets.release(&uri);
    assert!(second.real_path.exists());
    assert_eq!(db.get_app_file(&imported.id).unwrap().ref_count, 1);
    assets.release(&uri);
    assert!(!imported.real_path.exists());
    server.shutdown().await;
}

#[tokio::test]
#[ignore]
async fn measure_loopback_note_page_latency() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plain.db");
    let db = Db::open(&path).unwrap();
    let at = "2026-10-03T00:00:00.000Z";
    for index in 0..1000 {
        db.note_save(
            &format!("note-{index:04}"),
            "Title",
            &"content ".repeat(100),
            at,
        )
        .unwrap();
    }
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap());
    let token = crate::base64_encode(&[11; 32]);
    let server = ContentServer::start(&path, &token, prefs).unwrap();
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/graphql", server.port);
    let body = serde_json::json!({"query":"query { notes(offset:0,limit:50,query:\"\") {id title content createdAt updatedAt deletedAt} }"}).to_string();
    let mut direct = Vec::new();
    let mut http = Vec::new();
    for index in 0..110 {
        let start = std::time::Instant::now();
        assert_eq!(db.notes_list("", 50, 0).unwrap().len(), 50);
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        let start = std::time::Instant::now();
        let response = client
            .post(&url)
            .bearer_auth(&token)
            .header("content-type", "application/json")
            .body(body.clone())
            .send()
            .await
            .unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
        assert_eq!(value["data"]["notes"].as_array().unwrap().len(), 50);
        if index >= 10 {
            direct.push(elapsed);
            http.push(start.elapsed().as_secs_f64() * 1000.0);
        }
    }
    direct.sort_by(f64::total_cmp);
    http.sort_by(f64::total_cmp);
    println!(
        "1000 notes, 50-row page, 100 warm requests on macOS: direct SQLite p50={:.3}ms p95={:.3}ms; HTTP+GraphQL+JSON p50={:.3}ms p95={:.3}ms",
        direct[49], direct[94], http[49], http[94]
    );
    server.shutdown().await;
}

#[path = "clipboard.rs"]
mod clipboard;
