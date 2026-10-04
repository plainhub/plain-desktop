use super::*;

#[tokio::test]
async fn authenticated_runtime_preserves_radio_modes_and_releases_global_owner() {
    use std::sync::atomic::AtomicBool;
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    prefs.set("client_id", "fixture-mdns-root").unwrap();
    prefs
        .set("mdns_hostname", "fixture-mdns-root.local")
        .unwrap();
    prefs.set_user("https_port", 2443).unwrap();
    let token = crate::base64_encode(&[5; 32]);
    let server =
        super::super::ContentServer::start(&dir.path().join("plain.db"), &token, prefs.clone())
            .unwrap();
    let state = server.runtime_state();
    let (generation, mut rx) = state.host.connect();
    let allowed = Arc::new(AtomicBool::new(false));
    let permission = allowed.clone();
    let acquired = Arc::new(AtomicBool::new(false));
    let lease = acquired.clone();
    let host = state.host.clone();
    let responder = tokio::spawn(async move {
        while let Some(request) = rx.recv().await {
            let result = match request["method"].as_str().unwrap() {
                "mdnsMulticast" => {
                    let acquire = request["params"]["acquire"].as_bool().unwrap();
                    let ok = !acquire || permission.load(Ordering::SeqCst);
                    if ok {
                        lease.store(acquire, Ordering::SeqCst);
                    }
                    json!(ok)
                }
                "discoveryFacts" => {
                    json!({"name":"Actual","deviceType":"PHONE","version":"1","platform":"ANDROID","ips":["192.0.2.7"],"awareSupported":false,"awareRunning":false})
                }
                method => panic!("Unexpected host method: {method}"),
            };
            host.reply(generation, json!({"id":request["id"],"result":result}))
                .unwrap();
        }
    });
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/chat/mdns", server.port);
    assert_eq!(
        client
            .post(&url)
            .header("content-type", "application/json")
            .body(json!({"action":"snapshot"}).to_string())
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
            .body(json!({"action":"start","hostname":"spoofed"}).to_string())
            .send()
            .await
            .unwrap()
            .status(),
        422
    );
    let call = |action| {
        client
            .post(&url)
            .bearer_auth(&token)
            .header("content-type", "application/json")
            .body(json!({"action":action}).to_string())
    };
    assert_eq!(call("start").send().await.unwrap().status(), 400);
    assert!(!state.mdns.snapshot()["receiver"].as_bool().unwrap());
    assert!(!acquired.load(Ordering::SeqCst));
    allowed.store(true, Ordering::SeqCst);
    state
        .nearby_devices
        .scanning_modes(None, Some(true), vec![])
        .unwrap();
    let start: Value = parse(call("start").send().await.unwrap()).await;
    assert_eq!(start["result"]["receiver"], true);
    assert_eq!(start["result"]["scanning"], true);
    assert!(acquired.load(Ordering::SeqCst));
    let stop: Value = parse(call("stop").send().await.unwrap()).await;
    assert_eq!(stop["result"]["scanning"], false);
    assert!(
        state.nearby_devices.active(),
        "Stopping LAN must preserve active BLE"
    );
    state
        .nearby_devices
        .scanning_modes(None, Some(false), vec![])
        .unwrap();
    assert!(!state.nearby_devices.active());
    let published: Value = parse(call("publish").send().await.unwrap()).await;
    assert_eq!(published["result"]["published"], true);
    prefs.set("mdns_hostname", "fixture-new.local").unwrap();
    let updated: Value = parse(call("update").send().await.unwrap()).await;
    assert_eq!(updated["result"]["published"], true);
    assert_eq!(
        *state
            .mdns
            .session
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .hostname
            .read()
            .unwrap(),
        "fixture-new.local"
    );
    let withdrawn: Value = parse(call("unpublish").send().await.unwrap()).await;
    assert_eq!(withdrawn["result"]["published"], false);
    let updated: Value = parse(call("update").send().await.unwrap()).await;
    assert_eq!(
        updated["result"]["published"], false,
        "Update cannot republish an inactive service"
    );
    let other = Arc::new(Runtime::default());
    assert!(other.execute(&state, Request::Receiver {}).await.is_err());
    server.shutdown().await;
    assert!(!acquired.load(Ordering::SeqCst));
    assert_eq!(state.mdns.snapshot()["receiver"], false);
    assert!(OWNER.lock().await.is_none());
    assert!(!host_responder::is_running());
    state.host.disconnect(generation);
    responder.abort();
}

async fn parse(response: reqwest::Response) -> Value {
    assert!(response.status().is_success());
    serde_json::from_str(&response.text().await.unwrap()).unwrap()
}
