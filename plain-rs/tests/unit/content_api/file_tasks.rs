use super::*;
use crate::content_api::ContentServer;
use crate::filesystem::tasks::Store;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite_test::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
async fn call(port: u16, token: &str, doc: String) -> Value {
    let text = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/graphql"))
        .bearer_auth(token)
        .header("content-type", "application/json")
        .body(json!({"query":doc}).to_string())
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
#[tokio::test]
async fn mobile_http_tasks_persist_resolved_paths_and_gate_execution_on_native_authorization() {
    let temp = tempfile::tempdir().unwrap();
    let token = crate::base64_encode(&[12; 32]);
    let db_path = temp.path().join("tasks.db");
    let server = ContentServer::start(
        &db_path,
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
    let revoked = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let denied = revoked.clone();
    let scanned = Arc::new(Mutex::new(Vec::<String>::new()));
    let records = scanned.clone();
    let worker = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = socket.next().await {
            let request: Value = serde_json::from_str(&text).unwrap();
            let reply = if denied.load(std::sync::atomic::Ordering::SeqCst) {
                json!({"id":request["id"],"error":"synthetic permission revoked"})
            } else {
                match request["method"].as_str().unwrap() {
                    "fileTaskAuthorize" => {}
                    "fileTaskScan" => records.lock().unwrap().extend(
                        request["params"]["paths"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|value| value.as_str().unwrap().to_owned()),
                    ),
                    method => panic!("unexpected method {method}"),
                }
                json!({"id":request["id"],"result":true})
            };
            socket
                .send(Message::Text(reply.to_string().into()))
                .await
                .unwrap();
        }
    });
    let src = temp.path().join("source.txt");
    let dst = temp.path().join("target.txt");
    std::fs::write(&src, b"synthetic").unwrap();
    std::fs::write(&dst, b"existing").unwrap();
    let document = format!(
        "mutation {{ createFileHostTask(clientId:\"owner\",type:COPY,title:\"copy\",ops:[{{src:{},dst:{},overwrite:false}}]) {{ id status }} }}",
        json!(src.to_str().unwrap()),
        json!(dst.to_str().unwrap())
    );
    let queued = call(server.port, &token, document.clone()).await;
    assert!(queued.get("errors").is_none(), "{queued}");
    let id = queued["data"]["createFileHostTask"]["id"].as_str().unwrap();
    let done = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let result = call(server.port,&token,format!("query {{ fileHostTaskRecord(clientId:\"owner\",id:{}) {{ id status error doneBytes completedOps {{ src dst }} }} }}",json!(id))).await;
            assert!(result.get("errors").is_none(),"{result}");
            let task = result["data"]["fileHostTaskRecord"].clone();
            if task["status"] == "DONE" || task["status"] == "ERROR" { break task; }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }).await.unwrap();
    assert_eq!(done["status"], "DONE");
    let resolved = temp.path().join("target_1.txt");
    assert_eq!(done["completedOps"][0]["dst"], resolved.to_str().unwrap());
    assert!(
        scanned
            .lock()
            .unwrap()
            .contains(&resolved.to_str().unwrap().to_owned())
    );
    assert_eq!(std::fs::read(&resolved).unwrap(), b"synthetic");
    assert_eq!(std::fs::read(&dst).unwrap(), b"existing");
    let foreign = call(
        server.port,
        &token,
        format!(
            "query {{ fileHostTaskRecord(clientId:\"foreign\",id:{}) {{ id }} }}",
            json!(id)
        ),
    )
    .await;
    assert!(foreign["data"]["fileHostTaskRecord"].is_null());
    revoked.store(true, std::sync::atomic::Ordering::SeqCst);
    let denied = call(server.port, &token, document).await;
    assert!(denied.get("errors").is_some());
    assert!(!temp.path().join("target_2.txt").exists());
    let rows = tasks::sqlite::SqliteStore(Arc::new(Db::open(&db_path).unwrap()))
        .list("owner")
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, FileTaskStatus::Done);
    let removed = call(
        server.port,
        &token,
        format!(
            "mutation {{ removeFileHostTask(clientId:\"owner\",id:{}) }}",
            json!(id)
        ),
    )
    .await;
    assert_eq!(removed["data"]["removeFileHostTask"], true);
    worker.abort();
    let _ = worker.await;
    server.shutdown().await;
}
