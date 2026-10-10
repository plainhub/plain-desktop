use super::*;
#[test]
fn request_contract_requires_os_receipt_and_full_intent() {
    assert!(serde_json::from_value::<Request>(json!({"action":"receipt","id":"id","generation":"g","ticket":"t","receipt":{"path":"/saved","bytes":3,"error":null}})).is_ok());
    assert!(serde_json::from_value::<Request>(json!({"action":"receipt","id":"id","generation":"g","ticket":"t","receipt":{"path":"/saved","error":null}})).is_err());
    assert!(
        serde_json::from_value::<Request>(
            json!({"action":"control","id":"id","command":"pause","unexpected":true})
        )
        .is_err()
    );
}
async fn request(client: &reqwest::Client, url: &str, token: &str, body: Value) -> Value {
    let response = client
        .post(url)
        .bearer_auth(token)
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success(), "{}", response.status());
    serde_json::from_str::<Value>(&response.text().await.unwrap()).unwrap()["result"].clone()
}
async fn command(
    host: &super::super::host::Host,
    host_generation: u64,
    receiver: &mut tokio::sync::mpsc::Receiver<Value>,
    method: &str,
) -> Value {
    loop {
        let value = tokio::time::timeout(Duration::from_secs(5), receiver.recv())
            .await
            .unwrap()
            .unwrap();
        if value["method"] == method {
            return value;
        }
        host.reply(host_generation, json!({"id":value["id"],"result":true}))
            .unwrap();
    }
}
#[tokio::test]
async fn actual_http_requires_bearer_and_os_receipts_reject_short_reads_and_cancelled_generations()
{
    let directory = tempfile::tempdir().unwrap();
    let prefs = Arc::new(crate::prefs::Prefs::load(&directory.path().join("system.json")).unwrap());
    let token = crate::base64_encode(&[9; 32]);
    let server =
        crate::content_api::ContentServer::start(&directory.path().join("plain.db"), &token, prefs)
            .unwrap();
    let state = server.runtime_state();
    let (host_generation, mut receiver) = state.host.connect();
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/shares/batch", server.port);
    assert_eq!(
        client
            .post(&url)
            .header("content-type", "application/json")
            .body(json!({"action":"snapshot"}).to_string())
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let intent = json!({"message_id":"message","kind":"FILE","link":{"host":"127.0.0.1","port":8443,"sharedId":"fixture","token":crate::base64_encode(&[7;32]),"pageUrl":""},"url_token":crate::base64_encode(&[8;32]),"entries":[{"name":"file.txt","virtualPath":"file.txt","isDir":false,"size":3,"mimeType":"text/plain","hasThumb":false}],"target_dir":"/downloads","downloads_base":"/downloads","zip_name":""});
    let id = request(
        &client,
        &url,
        &token,
        json!({"action":"enqueue","intent":intent}),
    )
    .await;
    let first = command(
        &state.host,
        host_generation,
        &mut receiver,
        "sharedTransferStart",
    )
    .await;
    assert_eq!(
        first["params"]["operation"]["target"]["writeDir"],
        "/downloads"
    );
    state
        .host
        .reply(host_generation, json!({"id":first["id"],"result":true}))
        .unwrap();
    let p = &first["params"];
    assert_eq!(request(&client,&url,&token,json!({"action":"receipt","id":id,"generation":p["generation"],"ticket":p["ticket"],"receipt":{"path":"/downloads/file.txt","bytes":2,"error":null}})).await,false);
    let cleanup = command(
        &state.host,
        host_generation,
        &mut receiver,
        "sharedTransferCancel",
    )
    .await;
    state
        .host
        .reply(host_generation, json!({"id":cleanup["id"],"result":true}))
        .unwrap();
    let cleanup = command(
        &state.host,
        host_generation,
        &mut receiver,
        "sharedTransferCancel",
    )
    .await;
    state
        .host
        .reply(host_generation, json!({"id":cleanup["id"],"result":true}))
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let s = request(&client, &url, &token, json!({"action":"snapshot"})).await;
            if s["tasks"][0]["status"] == "FAILED" {
                assert_eq!(s["tasks"][0]["doneFiles"], 0);
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        request(
            &client,
            &url,
            &token,
            json!({"action":"control","id":id,"command":"retry"})
        )
        .await,
        true
    );
    let mut second = command(
        &state.host,
        host_generation,
        &mut receiver,
        "sharedTransferStart",
    )
    .await;
    assert_ne!(p["generation"], second["params"]["generation"]);
    state
        .host
        .reply(host_generation, json!({"id":second["id"],"result":true}))
        .unwrap();
    assert_eq!(request(&client,&url,&token,json!({"action":"receipt","id":id,"generation":p["generation"],"ticket":p["ticket"],"receipt":{"path":"/downloads/file.txt","bytes":3,"error":null}})).await,false);
    let pause_client = client.clone();
    let pause_url = url.clone();
    let pause_token = token.clone();
    let pause_id = id.clone();
    let pause = tokio::spawn(async move {
        pause_client
            .post(&pause_url)
            .bearer_auth(&pause_token)
            .header("content-type", "application/json")
            .body(json!({"action":"control","id":pause_id,"command":"pause"}).to_string())
            .send()
            .await
            .unwrap()
            .status()
    });
    let cleanup = command(
        &state.host,
        host_generation,
        &mut receiver,
        "sharedTransferCancel",
    )
    .await;
    state
        .host
        .reply(host_generation, json!({"id":cleanup["id"],"result":false}))
        .unwrap();
    assert_eq!(pause.await.unwrap(), StatusCode::BAD_REQUEST);
    assert_eq!(
        request(&client, &url, &token, json!({"action":"snapshot"})).await["tasks"][0]["status"],
        "PAUSED"
    );
    let resume_client = client.clone();
    let resume_url = url.clone();
    let resume_token = token.clone();
    let resume_id = id.clone();
    let resume = tokio::spawn(async move {
        request(
            &resume_client,
            &resume_url,
            &resume_token,
            json!({"action":"control","id":resume_id,"command":"resume"}),
        )
        .await
    });
    let cleanup = command(
        &state.host,
        host_generation,
        &mut receiver,
        "sharedTransferCancel",
    )
    .await;
    state
        .host
        .reply(host_generation, json!({"id":cleanup["id"],"result":true}))
        .unwrap();
    assert_eq!(resume.await.unwrap(), true);
    let third = command(
        &state.host,
        host_generation,
        &mut receiver,
        "sharedTransferStart",
    )
    .await;
    assert_ne!(
        third["params"]["generation"],
        second["params"]["generation"]
    );
    state
        .host
        .reply(host_generation, json!({"id":third["id"],"result":true}))
        .unwrap();
    second = third;
    let control_client = client.clone();
    let control_url = url.clone();
    let control_token = token.clone();
    let control_id = id.clone();
    let cancel = tokio::spawn(async move {
        request(
            &control_client,
            &control_url,
            &control_token,
            json!({"action":"control","id":control_id,"command":"cancel"}),
        )
        .await
    });
    let cleanup = command(
        &state.host,
        host_generation,
        &mut receiver,
        "sharedTransferCancel",
    )
    .await;
    state
        .host
        .reply(host_generation, json!({"id":cleanup["id"],"result":true}))
        .unwrap();
    assert_eq!(cancel.await.unwrap(), true);
    let s = request(&client, &url, &token, json!({"action":"snapshot"})).await;
    assert_eq!(s["tasks"][0]["status"], "CANCELED");
    let p = &second["params"];
    assert_eq!(request(&client,&url,&token,json!({"action":"receipt","id":id,"generation":p["generation"],"ticket":p["ticket"],"receipt":{"path":"/downloads/file.txt","bytes":3,"error":null}})).await,false);
    server.shutdown().await;
}
