use super::*;

#[tokio::test]
async fn file_import_dedupe_release_and_restart_use_core_storage() {
    let dir = tempfile::tempdir().unwrap();
    let token = crate::base64_encode(&[14; 32]);
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("system_prefs.json")).unwrap());
    let path = dir.path().join("plain.db");
    let server = ContentServer::start(&path, &token, prefs.clone()).unwrap();
    let source = dir.path().join("upload");
    std::fs::write(&source, b"shared attachment").unwrap();
    let query = format!(
        r#"mutation {{importAppFile(source:{},fileName:"original.%_",mimeType:"text/plain",deleteSource:false) {{id refCount realPath weakHash createdAt size}}}}"#,
        serde_json::to_string(source.to_str().unwrap()).unwrap()
    );
    let first = call(server.port, &token, &query).await;
    assert!(first.get("errors").is_none(), "{first}");
    let file = &first["data"]["importAppFile"];
    let id = file["id"].as_str().unwrap();
    let relative = file["realPath"].as_str().unwrap();
    assert_eq!(file["size"], 17);
    assert_eq!(
        call(server.port, &token, &query).await["data"]["importAppFile"]["refCount"],
        2
    );
    let page = call(server.port,&token,r#"{appFiles(offset:0,limit:20,query:"%_") {id} appFileCount(query:"%_") appFileRecords(offset:0,limit:20,query:"%_") {id refCount fileName}}"#).await;
    assert_eq!(page["data"]["appFileCount"], 1);
    assert_eq!(page["data"]["appFiles"][0]["id"], id);
    assert_eq!(page["data"]["appFileRecords"][0]["refCount"], 2);
    let suffix = Path::new(relative).file_name().unwrap().to_str().unwrap();
    let resolved = call(
        server.port,
        &token,
        &format!(
            "{{resolveAppFile(id:{})}}",
            serde_json::to_string(suffix).unwrap()
        ),
    )
    .await;
    assert_eq!(
        resolved["data"]["resolveAppFile"],
        dir.path().join(relative).to_str().unwrap()
    );
    for bad in ["../../secret", id] {
        assert!(
            call(
                server.port,
                &token,
                &format!(
                    "{{resolveAppFile(id:{})}}",
                    serde_json::to_string(bad).unwrap()
                )
            )
            .await
            .get("errors")
            .is_some()
        );
    }
    let url_token = crate::prefs::ensure_url_token(&prefs);
    let encoded = crate::base64_encode(
        &crate::xchacha_encrypt(
            &url_token,
            dir.path().join(relative).to_str().unwrap().as_bytes(),
        )
        .unwrap(),
    );
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/fs", server.port);
    let response = client
        .get(&url)
        .query(&[("id", &encoded)])
        .bearer_auth(&token)
        .header("range", "bytes=2-7")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 206);
    assert_eq!(response.headers()["content-range"], "bytes 2-7/17");
    assert_eq!(response.bytes().await.unwrap().as_ref(), b"ared a");
    let response = client
        .get(&url)
        .query(&[("id", &encoded)])
        .bearer_auth(&token)
        .header("range", "bytes=100-")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 416);
    server.shutdown().await;
    let server = ContentServer::start(&path, &token, prefs).unwrap();
    let release = format!(r#"mutation {{releaseAppFile(id:"{id}")}}"#);
    assert_eq!(
        call(server.port, &token, &release).await["data"]["releaseAppFile"],
        true
    );
    assert!(dir.path().join(relative).exists());
    assert_eq!(
        call(server.port, &token, &release).await["data"]["releaseAppFile"],
        true
    );
    assert!(!dir.path().join(relative).exists());
    assert_eq!(
        call(server.port, &token, &release).await["data"]["releaseAppFile"],
        false
    );
    assert!(source.exists());
    let moved = call(
        server.port,
        &token,
        &query.replace("deleteSource:false", "deleteSource:true"),
    )
    .await;
    assert!(moved.get("errors").is_none(), "{moved}");
    assert!(!source.exists());
    assert!(dir.path().join(relative).exists());
    server.shutdown().await;
}
