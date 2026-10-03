use crate::content_api::ContentServer;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tokio_tungstenite_test::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
#[tokio::test]
async fn reads_authorize_effective_parent_and_own_pagination_count_and_zip_filtering() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("allowed");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("a.txt"), b"1234").unwrap();
    std::fs::write(root.join("b.txt"), b"12").unwrap();
    let denied = temp.path().join("denied");
    std::fs::create_dir(&denied).unwrap();
    std::fs::write(denied.join("private.txt"), b"private").unwrap();
    let token = crate::base64_encode(&[28; 32]);
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
    let paths = Arc::new(Mutex::new(Vec::<String>::new()));
    let facts = paths.clone();
    let forbidden = denied.to_string_lossy().into_owned();
    let worker = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = socket.next().await {
            let request: Value = serde_json::from_str(&text).unwrap();
            let path = request["params"]["paths"][0].as_str().unwrap();
            let reply = if request["method"] == "fileTaskAuthorize" {
                facts.lock().unwrap().push(path.into());
                if path == forbidden {
                    json!({"id":request["id"],"error":"synthetic access denied"})
                } else {
                    json!({"id":request["id"],"result":true})
                }
            } else {
                assert_eq!(request["method"], "fileTaskZipEntries");
                let rows:Vec<_> = [("small",2),("large",8)].into_iter().map(|(name,size)| json!({"name":name,"path":format!("{path}{name}"),"permission":"","createdAt":null,"updatedAt":0,"size":size,"isDir":false,"childCount":0})).collect();
                json!({"id":request["id"],"result":rows})
            };
            socket
                .send(Message::Text(reply.to_string().into()))
                .await
                .unwrap();
        }
    });
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/files/read", server.port);
    let mut body = json!({"root":root,"query":"","text":null,"showHidden":null,"sortBy":"NAME_ASC","offset":1,"limit":1,"countOnly":false});
    assert_eq!(
        client
            .post(&url)
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    async fn call(client: &reqwest::Client, url: &str, token: &str, body: &Value) -> (u16, Value) {
        let response = client
            .post(url)
            .bearer_auth(token)
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .unwrap();
        let status = response.status().as_u16();
        let result = serde_json::from_str(&response.text().await.unwrap()).unwrap();
        (status, result)
    }
    let (status, page) = call(&client, &url, &token, &body).await;
    assert_eq!(status, 200);
    assert_eq!(page["count"], 2);
    assert_eq!(page["items"][0]["name"], "b.txt");
    body["countOnly"] = json!(true);
    let (_, page) = call(&client, &url, &token, &body).await;
    assert_eq!(page["count"], 2);
    assert!(page["items"].as_array().unwrap().is_empty());
    body["query"] = json!(format!("parent:{}", json!(denied.to_str().unwrap())));
    let (status, error) = call(&client, &url, &token, &body).await;
    assert_eq!(status, 400);
    assert!(error["error"].as_str().unwrap().contains("denied"));
    assert_eq!(
        paths.lock().unwrap().last().unwrap(),
        denied.to_str().unwrap()
    );
    let archive = temp.path().join("archive.zip");
    body["root"] = json!(format!("{}!zip!/", archive.display()));
    body["query"] = json!("file_size:>4");
    body["countOnly"] = json!(false);
    body["offset"] = json!(0);
    let (status, page) = call(&client, &url, &token, &body).await;
    assert_eq!(status, 200);
    assert_eq!(page["count"], 1);
    assert_eq!(page["items"][0]["name"], "large");
    assert_eq!(
        paths.lock().unwrap().last().unwrap(),
        archive.to_str().unwrap()
    );
    let stat_url = format!("http://127.0.0.1:{}/files/stat", server.port);
    let (status, record) = call(
        &client,
        &stat_url,
        &token,
        &json!({"path":root.join("a.txt")}),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(record["file"]["size"], 4);
    let (_, record) = call(&client, &stat_url, &token, &json!({"path":denied})).await;
    assert!(record["file"].is_null());
    let (_, record) = call(&client, &stat_url, &token, &json!({"path":"."})).await;
    assert!(record["file"].is_null());
    let (_, record) = call(
        &client,
        &stat_url,
        &token,
        &json!({"path":root.join("missing")}),
    )
    .await;
    assert!(record["file"].is_null());
    server.shutdown().await;
    worker.abort();
}
