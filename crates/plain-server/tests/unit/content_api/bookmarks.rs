use super::*;

#[tokio::test]
async fn bookmark_http_lifecycle_metadata_and_reference_counts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plain.db");
    let token = crate::base64_encode(&[10; 32]);
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap());
    let server = ContentServer::start(&path, &token, prefs.clone()).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let router = axum::Router::new()
        .route("/page", axum::routing::get(|| async { axum::response::Html("<title>fallback</title><meta content='Hello &amp; 🌍' property='og:title'><link href='icon' rel='shortcut icon'>") }))
        .route("/icon", axum::routing::get(|| async { ([("content-type", "image/x-icon")], vec![1u8,2,3,4]) }));
    let web = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let group = call(
        server.port,
        &token,
        "mutation {createBookmarkGroup(name:\"Links\") {id}}",
    )
    .await;
    let gid = group["data"]["createBookmarkGroup"]["id"].as_str().unwrap();
    let created = call(server.port, &token, &format!("mutation {{addBookmarks(urls:[\"{origin}/page\",\" \"],groupId:\"{gid}\") {{id title}}}}")).await;
    assert!(created.get("errors").is_none(), "{created}");
    assert_eq!(created["data"]["addBookmarks"].as_array().unwrap().len(), 1);
    let id = created["data"]["addBookmarks"][0]["id"].as_str().unwrap();
    let meta_query =
        format!("mutation {{fetchBookmarkMetadata(id:\"{id}\") {{title faviconPath}}}}");
    let meta = call(server.port, &token, &meta_query).await;
    assert!(meta.get("errors").is_none(), "{meta}");
    assert_eq!(meta["data"]["fetchBookmarkMetadata"]["title"], "Hello & 🌍");
    let icon = meta["data"]["fetchBookmarkMetadata"]["faviconPath"]
        .as_str()
        .unwrap();
    assert!(std::path::Path::new(icon).exists());
    assert!(
        call(server.port, &token, &meta_query).await["data"]["fetchBookmarkMetadata"].is_null()
    );
    let db = Arc::new(Db::open(&path).unwrap());
    let file_id = std::path::Path::new(icon)
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(db.get_app_file(file_id).unwrap().ref_count, 1);
    let second = call(
        server.port,
        &token,
        &format!("mutation {{addBookmarks(urls:[\"{origin}/page\"],groupId:\"{gid}\") {{id}}}}"),
    )
    .await;
    let second_id = second["data"]["addBookmarks"][0]["id"].as_str().unwrap();
    call(
        server.port,
        &token,
        &format!("mutation {{fetchBookmarkMetadata(id:\"{second_id}\") {{id}}}}"),
    )
    .await;
    assert_eq!(db.get_app_file(file_id).unwrap().ref_count, 2);

    let query = format!("mutation {{recordBookmarkClick(id:\"{id}\")}}");
    futures_util::future::join_all((0..20).map(|_| call(server.port, &token, &query))).await;
    assert_eq!(
        crate::db::bookmark::get_bookmark_by_id(&db, id)
            .unwrap()
            .click_count,
        20
    );
    let stale = crate::db::bookmark::get_bookmark_by_id(&db, id).unwrap();
    let edit = format!(
        "mutation {{updateBookmark(id:\"{id}\",input:{{url:\"{origin}/page\",title:\"Edited\",groupId:\"{gid}\",pinned:true,sortOrder:2}}) {{title clickCount}}}}"
    );
    assert_eq!(
        call(server.port, &token, &edit).await["data"]["updateBookmark"]["clickCount"],
        20
    );
    assert_eq!(
        crate::db::bookmark::update_metadata(&db, &stale, "stale", "bad").unwrap(),
        0
    );
    let groups = call(server.port, &token, "{bookmarkGroups {itemCount}}").await;
    assert_eq!(groups["data"]["bookmarkGroups"][0]["itemCount"], 2);
    call(
        server.port,
        &token,
        &format!("mutation {{deleteBookmarkGroup(id:\"{gid}\")}}"),
    )
    .await;
    assert_eq!(
        crate::db::bookmark::get_bookmark_by_id(&db, id)
            .unwrap()
            .group_id,
        ""
    );
    assert_eq!(
        call(
            server.port,
            &token,
            "mutation {deleteBookmarks(ids:[]) {affectedCount}}"
        )
        .await["data"]["deleteBookmarks"]["affectedCount"],
        0
    );
    server.shutdown().await;
    let server = ContentServer::start(&path, &token, prefs).unwrap();
    assert_eq!(
        call(server.port, &token, "{bookmarks {clickCount}}").await["data"]["bookmarks"][0]["clickCount"],
        20
    );
    assert_eq!(
        call(
            server.port,
            &token,
            &format!("mutation {{deleteBookmarks(ids:[\"{id}\"]) {{affectedCount}}}}")
        )
        .await["data"]["deleteBookmarks"]["affectedCount"],
        1
    );
    assert!(std::path::Path::new(icon).exists());
    assert_eq!(db.get_app_file(file_id).unwrap().ref_count, 1);
    call(
        server.port,
        &token,
        &format!("mutation {{deleteBookmarks(ids:[\"{second_id}\"]) {{affectedCount}}}}"),
    )
    .await;
    assert!(!std::path::Path::new(icon).exists());
    assert!(db.get_app_file(file_id).is_none());
    server.shutdown().await;
    web.abort();
}
