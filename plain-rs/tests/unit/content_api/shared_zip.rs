use super::*;
use std::io::Read;
#[test]
fn zip_roundtrip_validates_names_sources_and_cancellation_and_cleans_temporary() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source");
    std::fs::write(&source, "synthetic 中文").unwrap();
    let item = || Item {
        source_path: source.clone(),
        entry_name: "folder/中文 +?#%.txt".into(),
    };
    let (file, temporary) = pack(
        directory.path().join("zip"),
        vec![item()],
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    let path = temporary.0.clone();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let mut entry = archive.by_name("folder/中文 +?#%.txt").unwrap();
    let mut text = String::new();
    entry.read_to_string(&mut text).unwrap();
    assert_eq!(text, "synthetic 中文");
    drop(entry);
    drop(archive);
    drop(temporary);
    assert!(!path.exists());
    for name in ["/escape", "../escape", "a/../b", "a\\b", "a//b", ""] {
        assert!(
            pack(
                directory.path().join("zip"),
                vec![Item {
                    entry_name: name.into(),
                    ..item()
                }],
                Arc::new(AtomicBool::new(false))
            )
            .is_err()
        );
    }
    assert!(
        pack(
            directory.path().join("zip"),
            vec![item(), item()],
            Arc::new(AtomicBool::new(false))
        )
        .is_err()
    );
    assert!(
        pack(
            directory.path().join("zip"),
            vec![item()],
            Arc::new(AtomicBool::new(true))
        )
        .is_err()
    );
    assert_eq!(
        std::fs::read_dir(directory.path().join("zip"))
            .unwrap()
            .count(),
        0
    );
}
#[tokio::test]
async fn authenticated_zip_http_streams_root_archive_without_native_host() {
    let directory = tempfile::tempdir().unwrap();
    let prefs = Arc::new(crate::prefs::Prefs::load(&directory.path().join("system.json")).unwrap());
    let token = crate::base64_encode(&[4; 32]);
    let server =
        crate::content_api::ContentServer::start(&directory.path().join("plain.db"), &token, prefs)
            .unwrap();
    let source = directory.path().join("source");
    std::fs::write(&source, vec![7; 256 * 1024]).unwrap();
    let request = json!({"items":[{"sourcePath":source,"entryName":"root/子目录/a.txt"}]});
    let url = format!("http://127.0.0.1:{}/shares/client/zip", server.port);
    let client = reqwest::Client::new();
    assert_eq!(
        client
            .post(&url)
            .header("content-type", "application/json")
            .body(request.to_string())
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let response = client
        .post(&url)
        .bearer_auth(&token)
        .header("content-type", "application/json")
        .body(request.to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "application/zip");
    let bytes = response.bytes().await.unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut content = Vec::new();
    archive
        .by_name("root/子目录/a.txt")
        .unwrap()
        .read_to_end(&mut content)
        .unwrap();
    assert_eq!(content, vec![7; 256 * 1024]);
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert_eq!(
        std::fs::read_dir(directory.path().join("shared-zip"))
            .unwrap()
            .count(),
        0
    );
    server.shutdown().await;
}

#[tokio::test]
async fn dropping_archive_body_releases_capacity_and_deletes_root_spool() {
    let directory = tempfile::tempdir().unwrap();
    let prefs = Arc::new(crate::prefs::Prefs::load(&directory.path().join("system.json")).unwrap());
    let token = crate::base64_encode(&[4; 32]);
    let server =
        crate::content_api::ContentServer::start(&directory.path().join("plain.db"), &token, prefs)
            .unwrap();
    let state = server.runtime_state();
    let source = directory.path().join("source");
    std::fs::write(&source, vec![8; 128 * 1024]).unwrap();
    let mut headers = HeaderMap::new();
    headers.insert("authorization", format!("Bearer {token}").parse().unwrap());
    let response = call(
        State(state.clone()),
        headers,
        Json(Request {
            items: vec![Item {
                source_path: source,
                entry_name: "a.txt".into(),
            }],
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(state.lan.capacity.available_permits(), 3);
    assert_eq!(
        std::fs::read_dir(directory.path().join("shared-zip"))
            .unwrap()
            .count(),
        1
    );
    drop(response);
    assert_eq!(state.lan.capacity.available_permits(), 4);
    assert_eq!(
        std::fs::read_dir(directory.path().join("shared-zip"))
            .unwrap()
            .count(),
        0
    );
    server.shutdown().await;
}
