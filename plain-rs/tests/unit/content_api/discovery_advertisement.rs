use super::*;
#[cfg(feature = "http_transport")]
#[tokio::test]
async fn authenticated_http_uses_current_root_preferences_after_delayed_os_facts() {
    use std::sync::Arc;
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    prefs.set("client_id", "fixture-root").unwrap();
    prefs.set_user("device_name", "Before").unwrap();
    prefs.set_user("https_port", 2443).unwrap();
    prefs.set("mdns_hostname", "before.local").unwrap();
    let token = crate::base64_encode(&[3; 32]);
    let server =
        super::super::ContentServer::start(&dir.path().join("plain.db"), &token, prefs.clone())
            .unwrap();
    let state = server.runtime_state();
    let (generation, mut rx) = state.host.connect();
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/chat/discovery", server.port);
    let unauthorized = client
        .post(&url)
        .header("content-type", "application/json")
        .body(json!({"action":"reply"}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), 401);
    let unknown = client
        .post(&url)
        .bearer_auth(&token)
        .header("content-type", "application/json")
        .body(json!({"action":"reply","id":"spoofed"}).to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(unknown.status(), 422);
    let host = state.host.clone();
    let changed = prefs.clone();
    let responder = tokio::spawn(async move {
        for n in 0..3 {
            let req = rx.recv().await.unwrap();
            assert_eq!(req["method"], "discoveryFacts");
            assert_eq!(req["params"], json!({}));
            if n == 0 {
                changed.set_user("device_name", "Current 中文").unwrap();
                changed.set_user("https_port", 3443).unwrap();
                changed.set("mdns_hostname", "current.local").unwrap();
            }
            host.reply(generation,json!({"id":req["id"],"result":{"name":"Actual tablet","deviceType":"TABLET","version":"3.3.24","platform":"IOS","ips":["192.168.7.2"],"awareSupported":true,"awareRunning":true}})).unwrap();
        }
    });
    let call = |action| {
        client
            .post(&url)
            .bearer_auth(&token)
            .header("content-type", "application/json")
            .body(json!({"action":action}).to_string())
    };
    let reply: Value =
        serde_json::from_str(&call("reply").send().await.unwrap().text().await.unwrap()).unwrap();
    assert_eq!(reply["result"]["id"], "fixture-root");
    assert_eq!(reply["result"]["name"], "Current 中文");
    assert_eq!(reply["result"]["port"], 3443);
    assert_eq!(reply["result"]["deviceType"], "TABLET");
    let mdns: Value =
        serde_json::from_str(&call("mdns").send().await.unwrap().text().await.unwrap()).unwrap();
    assert_eq!(mdns["result"]["instanceName"], "Current 中文");
    assert_eq!(mdns["result"]["targetHostname"], "current.local");
    assert_eq!(mdns["result"]["txtRecords"][0], "id=fixture-root");
    assert_eq!(mdns["result"]["txtRecords"][4], "aw=1");
    let ble: Value =
        serde_json::from_str(&call("ble").send().await.unwrap().text().await.unwrap()).unwrap();
    let bytes = crate::base64_decode(ble["result"].as_str().unwrap());
    assert_eq!(
        bytes,
        advertisement::ble("fixture-root", true, true).unwrap()
    );
    responder.await.unwrap();
    state.host.disconnect(generation);
    server.shutdown().await;
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn ble_scan_decodes_raw_bytes_without_host_and_checks_request_contract() {
    let dir = tempfile::tempdir().unwrap();
    let prefs =
        std::sync::Arc::new(crate::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    let token = crate::base64_encode(&[2; 32]);
    let server =
        super::super::ContentServer::start(&dir.path().join("plain.db"), &token, prefs).unwrap();
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/chat/discovery", server.port);
    let call = |body: Value| {
        client
            .post(&url)
            .bearer_auth(&token)
            .header("content-type", "application/json")
            .body(body.to_string())
    };
    assert_eq!(
        client
            .post(&url)
            .header("content-type", "application/json")
            .body(json!({"action":"bleDecode","payload":null}).to_string())
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let result: Value = serde_json::from_str(
        &call(json!({"action":"bleDecode","payload":[243,0,1,2,3,4,128,254,255]}))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        result["result"],
        json!({"shortId":"000102030480feff","awareSupported":true,"awareRunning":true})
    );
    for payload in [Value::Null, json!([1, 2, 3])] {
        let row: Value = serde_json::from_str(
            &call(json!({"action":"bleDecode","payload":payload}))
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(row["result"], Value::Null);
    }
    let row: Value = serde_json::from_str(
        &call(json!({"action":"bleShortId","id":"fixture"}))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(row["result"], "f16d05ec6b29248d");
    assert_eq!(
        call(json!({"action":"bleDecode","payload":[-1]}))
            .send()
            .await
            .unwrap()
            .status(),
        422
    );
    assert_eq!(
        call(json!({"action":"bleShortId","id":"fixture","shortId":"spoof"}))
            .send()
            .await
            .unwrap()
            .status(),
        422
    );
    assert_eq!(
        call(json!({"action":"bleShortId","id":""}))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    assert_eq!(
        call(json!({"action":"bleDecode","payload":vec![0u8;1651]}))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    server.shutdown().await;
}
