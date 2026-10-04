use super::*;
use std::sync::Arc;
#[tokio::test]
async fn private_attachment_http_auth_owns_paths_and_returns_only_committed_receipts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plain.db");
    let db = crate::db::Db::open(&path).unwrap();
    let mut chat = crate::db::DChat::new("me","peer","",&json!({"type":"IMAGES","value":{"items":[{"id":"attachment","uri":"fsid:remote","fileName":"photo.png","size":3}]}}).to_string());
    chat.id = "message".into();
    db.insert_chat(&chat);
    let token = crate::base64_encode(&[7; 32]);
    let server = crate::content_api::ContentServer::start(
        &path,
        &token,
        Arc::new(crate::prefs::Prefs::load(&dir.path().join("system_prefs.json")).unwrap()),
    )
    .unwrap();
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/chat/attachment", server.port);
    let request =
        json!({"action":"begin","message_id":"message","id":"attachment","uri":"fsid:remote"});
    assert_eq!(
        client
            .post(&url)
            .header("content-type", "application/json")
            .body(request.to_string())
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        client
            .post(&url)
            .bearer_auth(&token)
            .header("content-type", "application/json")
            .body(json!({"action":"finish","token":"invalid","path":"/untrusted"}).to_string())
            .send()
            .await
            .unwrap()
            .status(),
        422
    );
    let ticket = client
        .post(&url)
        .bearer_auth(&token)
        .header("content-type", "application/json")
        .body(request.to_string())
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let ticket: serde_json::Value = serde_json::from_str(&ticket).unwrap();
    let ticket = &ticket["result"];
    let source = ticket["path"].as_str().unwrap();
    std::fs::write(source, b"abc").unwrap();
    let result = client
        .post(&url)
        .bearer_auth(&token)
        .header("content-type", "application/json")
        .body(json!({"action":"finish","token":ticket["token"]}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(result.status(), 200);
    let result: serde_json::Value = serde_json::from_str(&result.text().await.unwrap()).unwrap();
    assert!(
        result["result"]["uri"]
            .as_str()
            .unwrap()
            .starts_with("fid:")
    );
    assert_eq!(
        std::fs::read(result["result"]["path"].as_str().unwrap()).unwrap(),
        b"abc"
    );
    assert!(
        result["result"]["chat"]["content"]
            .as_str()
            .unwrap()
            .contains("fid:")
    );
    assert!(!std::path::Path::new(source).exists());
    assert_eq!(
        client
            .post(&url)
            .bearer_auth(&token)
            .header("content-type", "application/json")
            .body(json!({"action":"finish","token":ticket["token"]}).to_string())
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    drop(server);
}
