use crate::content_api::ContentServer;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
#[tokio::test]
async fn strict_rename_rebinds_and_delete_cleans_confirmed_metadata_through_shared_host_services() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source.png");
    let target = temp.path().join("target.png");
    std::fs::write(&source, b"synthetic").unwrap();
    std::fs::write(&target, b"existing").unwrap();
    let token = crate::base64_encode(&[37; 32]);
    let path = temp.path().join("content.db");
    let server = ContentServer::start(
        &path,
        &token,
        Arc::new(crate::prefs::Prefs::load(&temp.path().join("prefs.json")).unwrap()),
    )
    .unwrap();
    let db = crate::db::Db::open(&path).unwrap();
    db.with_conn(|c|{c.execute("INSERT INTO tag_relations(tag_id,key,type,created_at,size,title) VALUES('image','old',3,'first',9,'image')",[])?;c.execute("INSERT INTO tag_relations(tag_id,key,type,created_at,size,title) VALUES('file',?1,22,'first',9,'file')",[source.to_str().unwrap()])?;Ok::<_,rusqlite::Error>(())}).unwrap();
    let mut request = format!("ws://127.0.0.1:{}/host", server.port)
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    let (mut socket, _) = connect_async(request).await.unwrap();
    let worker = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = socket.next().await {
            let request: Value = serde_json::from_str(&text).unwrap();
            let result=match request["method"].as_str().unwrap() {
                "fileTaskAuthorize"|"fileTaskScan"=>json!(true),
                "fileTaskMediaSnapshot"=>Value::Array(request["params"]["paths"].as_array().unwrap().iter().filter_map(|p|{
                    let path=p.as_str().unwrap();if !std::path::Path::new(path).is_file(){return None;}
                    Some(json!({"mediaType":3,"mediaId":if path.ends_with("source.png"){"old"}else{"new"},"path":path}))
                }).collect()),method=>panic!("unexpected method {method}"),
            };
            socket
                .send(Message::Text(
                    json!({"id":request["id"],"result":result})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
        }
    });
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/files/mutate", server.port);
    async fn call(client: &reqwest::Client, url: &str, token: &str, body: Value) -> Value {
        let response = client
            .post(url)
            .bearer_auth(token)
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
        serde_json::from_str(&response.text().await.unwrap()).unwrap()
    }
    let result = call(
        &client,
        &url,
        &token,
        json!({"action":"rename","path":source,"name":"target.png"}),
    )
    .await;
    assert!(result["path"].is_null());
    assert_eq!(std::fs::read(&target).unwrap(), b"existing");
    assert!(source.exists());
    let result = call(
        &client,
        &url,
        &token,
        json!({"action":"rename","path":source,"name":"../escape.png"}),
    )
    .await;
    assert!(result["path"].is_null());
    assert!(source.exists());
    let renamed = temp.path().join("renamed.png");
    let result = call(
        &client,
        &url,
        &token,
        json!({"action":"rename","path":source,"name":"renamed.png"}),
    )
    .await;
    assert_eq!(result["path"], renamed.to_str().unwrap());
    assert!(!source.exists());
    db.with_conn(|c| {
        assert_eq!(
            c.query_row(
                "SELECT key FROM tag_relations WHERE tag_id='image'",
                [],
                |r| r.get::<_, String>(0)
            )?,
            "new"
        );
        assert_eq!(
            c.query_row(
                "SELECT key FROM tag_relations WHERE tag_id='file'",
                [],
                |r| r.get::<_, String>(0)
            )?,
            renamed.to_str().unwrap()
        );
        Ok::<_, rusqlite::Error>(())
    })
    .unwrap();
    let result = call(
        &client,
        &url,
        &token,
        json!({"action":"delete","path":renamed}),
    )
    .await;
    assert_eq!(result["removed"], true);
    assert!(!renamed.exists());
    assert_eq!(
        db.with_conn(
            |c| c.query_row("SELECT count(*) FROM tag_relations", [], |r| r
                .get::<_, i64>(0))
        )
        .unwrap(),
        0
    );
    let result = call(
        &client,
        &url,
        &token,
        json!({"action":"delete","path":renamed}),
    )
    .await;
    assert_eq!(result["removed"], false);
    server.shutdown().await;
    worker.abort();
}
