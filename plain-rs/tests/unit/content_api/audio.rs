use super::*;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite_test::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};

#[tokio::test]
async fn authenticated_host_resolves_large_library_and_counts_only_reported_starts() {
    let dir = tempfile::tempdir().unwrap();
    let token = crate::base64_encode(&[9; 32]);
    let server = ContentServer::start(
        &dir.path().join("db"),
        &token,
        Arc::new(crate::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap()),
    )
    .unwrap();
    let url = format!("ws://127.0.0.1:{}/host", server.port);
    assert!(connect_async(&url).await.is_err());
    let mut request = url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    let (mut socket, _) = connect_async(request).await.unwrap();
    let task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = socket.next().await {
            let request: Value = serde_json::from_str(&text).unwrap();
            let params = &request["params"];
            let index_of = |path: &str| {
                path.trim_start_matches("/audio/")
                    .parse::<usize>()
                    .ok()
                    .filter(|i| *i < 10_000)
            };
            let track = |i: usize| json!({"path":format!("/audio/{i}"),"title":format!("Track {i}"),"artist":"Synthetic","albumId":"42","durationMs":117123});
            let result = match request["method"].as_str().unwrap() {
                "audioLibraryCount" => json!(10_000),
                "audioLibraryContains" => {
                    json!(index_of(params["path"].as_str().unwrap()).is_some())
                }
                method => {
                    assert_eq!(params["sortBy"], "NAME_ASC");
                    match method {
                        "audioLibraryLocate" => json!(
                            index_of(params["path"].as_str().unwrap())
                                .map(|i| i as i64)
                                .unwrap_or(-1)
                        ),
                        "audioLibraryPath" => {
                            let i = params["offset"].as_u64().unwrap() as usize;
                            if i < 10_000 {
                                json!(format!("/audio/{i}"))
                            } else {
                                Value::Null
                            }
                        }
                        "audioLibraryPage" => {
                            let offset = params["offset"].as_u64().unwrap() as usize;
                            let limit = params["limit"].as_u64().unwrap() as usize;
                            assert!(limit <= 500);
                            json!(
                                (offset..(offset + limit).min(10_000))
                                    .map(track)
                                    .collect::<Vec<_>>()
                            )
                        }
                        _ => panic!("unknown method"),
                    }
                }
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
    let selected=call(server.port,&token,r#"mutation {audioHostSetLibrarySource(startPath:"/audio/9000",shuffle:false,sortBy:"NAME_ASC") {path durationMs albumId}}"#).await;
    assert!(selected.get("errors").is_none(), "{selected}");
    assert_eq!(
        selected["data"]["audioHostSetLibrarySource"]["durationMs"],
        117123
    );
    let history = call(
        server.port,
        &token,
        r#"query {audioHostHistory(offset:0,limit:20,query:"") {playCount}}"#,
    )
    .await;
    assert_eq!(history["data"]["audioHostHistory"], json!([]));
    let page = call(
        server.port,
        &token,
        r#"query {audioHostQueueItems(offset:8990,limit:20,query:"") {path}}"#,
    )
    .await;
    assert_eq!(
        page["data"]["audioHostQueueItems"]
            .as_array()
            .unwrap()
            .len(),
        20
    );
    assert_eq!(
        page["data"]["audioHostQueueItems"][19]["path"],
        "/audio/9009"
    );
    let next = call(
        server.port,
        &token,
        "mutation {audioHostResolveNext(isNext:true,shuffle:false) {path}}",
    )
    .await;
    assert_eq!(next["data"]["audioHostResolveNext"]["path"], "/audio/9001");
    let ack=call(server.port,&token,r#"mutation {audioHostOnPlaying(path:"/audio/9001",title:"Track",artist:"Synthetic",durationMs:117123)}"#).await;
    assert!(ack.get("errors").is_none(), "{ack}");
    let history = call(
        server.port,
        &token,
        r#"query {audioHostHistory(offset:0,limit:20,query:"") {playCount durationMs}}"#,
    )
    .await;
    assert_eq!(history["data"]["audioHostHistory"][0]["playCount"], 1);
    let enqueued=call(server.port,&token,r#"mutation {audioHostEnqueue(items:[{path:"/audio/9002",title:"Manual",artist:"Synthetic",albumId:"42",durationMs:117123}],playNext:false)}"#).await;
    assert!(enqueued.get("errors").is_none(), "{enqueued}");
    let next = call(
        server.port,
        &token,
        "mutation {audioHostResolveNext(isNext:true,shuffle:false) {path title}}",
    )
    .await;
    assert_eq!(next["data"]["audioHostResolveNext"]["path"], "/audio/9002");
    assert_eq!(next["data"]["audioHostResolveNext"]["title"], "Manual");
    task.abort();
    let _ = task.await;
    let query = call(server.port, &token, "query {audioHostQueueCount}").await;
    assert!(
        query.get("errors").is_some(),
        "missing host must not return a fake empty library: {query}"
    );
    server.shutdown().await;
}
