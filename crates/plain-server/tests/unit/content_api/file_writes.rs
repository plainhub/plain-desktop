use crate::content_api::ContentServer;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};

#[tokio::test]
async fn authenticated_writes_require_native_authorization_and_confirm_scans() {
    let temp = tempfile::tempdir().unwrap();
    let token = crate::base64_encode(&[19; 32]);
    let server = ContentServer::start(
        &temp.path().join("content.db"),
        &token,
        Arc::new(crate::prefs::Prefs::load(&temp.path().join("prefs.json")).unwrap()),
    )
    .unwrap();
    let mut request = format!("ws://127.0.0.1:{}/host", server.port)
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    let (mut socket, _) = connect_async(request).await.unwrap();
    let denied = Arc::new(AtomicBool::new(false));
    let gate = denied.clone();
    let scan_failed = Arc::new(AtomicBool::new(false));
    let scan_gate = scan_failed.clone();
    let scans = Arc::new(Mutex::new(Vec::<String>::new()));
    let records = scans.clone();
    let worker = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = socket.next().await {
            let request: Value = serde_json::from_str(&text).unwrap();
            let reply = if gate.load(Ordering::SeqCst) {
                json!({"id":request["id"],"error":"synthetic permission denied"})
            } else if request["method"] == "fileTaskScan" && scan_gate.load(Ordering::SeqCst) {
                json!({"id":request["id"],"result":false})
            } else {
                if request["method"] == "fileTaskScan" {
                    records
                        .lock()
                        .unwrap()
                        .push(request["params"]["paths"][0].as_str().unwrap().into());
                } else {
                    assert_eq!(request["method"], "fileTaskAuthorize");
                }
                json!({"id":request["id"],"result":true})
            };
            socket
                .send(Message::Text(reply.to_string().into()))
                .await
                .unwrap();
        }
    });
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/files/write", server.port);
    let path = temp.path().join("parent/child");
    let create = json!({"action":"createDirectory","path":path});
    assert_eq!(
        client
            .post(&url)
            .header("content-type", "application/json")
            .body(create.to_string())
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert!(!path.exists());
    let row = client
        .post(&url)
        .bearer_auth(&token)
        .header("content-type", "application/json")
        .body(create.to_string())
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let row: Value = serde_json::from_str(&row).unwrap();
    assert_eq!(row["isDir"], true);
    let file = path.join("中文.txt");
    let write =
        json!({"action":"writeText","path":file,"content":"synthetic 中文\n","overwrite":false});
    let row = client
        .post(&url)
        .bearer_auth(&token)
        .header("content-type", "application/json")
        .body(write.to_string())
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let row: Value = serde_json::from_str(&row).unwrap();
    assert_eq!(row["size"], "synthetic 中文\n".len());
    assert_eq!(row["name"], "中文.txt");
    assert!(row["updatedAt"].as_i64().unwrap() > 0);
    assert_eq!(
        client
            .post(&url)
            .bearer_auth(&token)
            .header("content-type", "application/json")
            .body(write.to_string())
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "synthetic 中文\n");
    denied.store(true, Ordering::SeqCst);
    let rejected = path.join("denied.txt");
    let response = client
        .post(&url)
        .bearer_auth(&token)
        .header("content-type", "application/json")
        .body(json!({"action":"createFile","path":rejected}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    assert!(!rejected.exists());
    assert_eq!(scans.lock().unwrap().len(), 2);
    denied.store(false, Ordering::SeqCst);
    scan_failed.store(true, Ordering::SeqCst);
    let unscanned = path.join("unscanned.txt");
    let response = client
        .post(&url)
        .bearer_auth(&token)
        .header("content-type", "application/json")
        .body(json!({"action":"createFile","path":unscanned}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    assert!(unscanned.is_file());
    assert_eq!(scans.lock().unwrap().len(), 2);
    server.shutdown().await;
    worker.abort();
}
