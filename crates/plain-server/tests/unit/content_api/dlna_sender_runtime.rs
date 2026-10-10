use super::*;
#[tokio::test]
async fn queue_mutations_preserve_legacy_duplicates_and_order() {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    let token = crate::base64_encode(&[3; 32]);
    let server =
        super::super::ContentServer::start(&dir.path().join("plain.db"), &token, prefs).unwrap();
    let client = reqwest::Client::new();
    let command = |body: serde_json::Value| {
        client
            .post(format!(
                "http://127.0.0.1:{}/system/dlna-sender",
                server.port
            ))
            .bearer_auth(&token)
            .header("content-type", "application/json")
            .body(body.to_string())
    };
    for path in ["a.mp3", "b.mp3", "a.mp3"] {
        let response = command(json!({"action":"add","item":{"path":path,"title":path}}))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
    }
    let row = command(json!({"action":"reorder","from":0,"to":1}))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let row: serde_json::Value = serde_json::from_str(&row).unwrap();
    assert_eq!(row["result"]["items"].as_array().unwrap().len(), 3);
    assert_eq!(row["result"]["items"][0]["path"], "b.mp3");
    let row = command(json!({"action":"removeAt","index":0}))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let row: serde_json::Value = serde_json::from_str(&row).unwrap();
    assert_eq!(row["result"]["items"].as_array().unwrap().len(), 2);
    assert_eq!(row["result"]["items"][0]["path"], "a.mp3");
    let row = command(json!({"action":"clear"}))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let row: serde_json::Value = serde_json::from_str(&row).unwrap();
    assert!(row["result"]["items"].as_array().unwrap().is_empty());
    server.shutdown().await;
}
#[test]
fn command_rejects_missing_fields_and_unknown_actions() {
    for body in [
        json!({"action":"select"}),
        json!({"action":"seek"}),
        json!({"action":"cast","item":{"path":"a"}}),
        json!({"action":"snapshot","nativeFallback":true}),
        json!({"action":"unknown"}),
    ] {
        assert!(serde_json::from_value::<Command>(body).is_err());
    }
}

#[tokio::test]
async fn sender_controls_use_soap_and_release_the_original_subscription() {
    use crate::dlna_sender::types::UpnpService;
    use axum::{Router, body::to_bytes, extract::Request, routing::any};
    let calls = Arc::new(Mutex::new(Vec::<(String, String, String)>::new()));
    let captured = calls.clone();
    let router = Router::new().fallback(any(move |request: Request| {
        let captured = captured.clone();
        async move {
            let method = request.method().as_str().to_owned();
            let action = request
                .headers()
                .get("soapaction")
                .and_then(|s| s.to_str().ok())
                .unwrap_or_default()
                .to_owned();
            let sid = request
                .headers()
                .get("sid")
                .and_then(|s| s.to_str().ok())
                .unwrap_or_default()
                .to_owned();
            let body = to_bytes(request.into_body(), 1024 * 1024).await.unwrap();
            captured.lock().unwrap().push((
                method.clone(),
                if action.is_empty() { sid } else { action },
                String::from_utf8(body.to_vec()).unwrap(),
            ));
            (
                [
                    ("SID", "uuid:test-subscription"),
                    ("TIMEOUT", "Second-3600"),
                ],
                "<Envelope><Body/></Envelope>",
            )
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let renderer = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    let server = super::super::ContentServer::start(
        &dir.path().join("plain.db"),
        &crate::base64_encode(&[3; 32]),
        prefs,
    )
    .unwrap();
    let state = server.runtime_state();
    let host = state.host.clone();
    let (generation, mut requests) = host.connect();
    let host_task = tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let id = request["id"].as_u64().unwrap();
            let result = match request["method"].as_str().unwrap() {
                "systemCastAddressFacts" => {
                    json!({"baseUrl":"http://127.0.0.1:7878", "senderName":"Test Phone"})
                }
                "systemFileResource" => json!("video/mp4"),
                method => panic!("Unexpected native operation: {method}"),
            };
            let _ = host.reply(generation, json!({"id":id,"result":result}));
        }
    });
    let device = DiscoveredDevice {
        udn: "uuid:renderer".into(),
        location: format!("{base}/description.xml"),
        has_av_transport: true,
        av_transport: UpnpService {
            service_type: "urn:schemas-upnp-org:service:AVTransport:1".into(),
            control_url: format!("{base}/control"),
            event_sub_url: format!("{base}/event"),
            ..Default::default()
        },
        ..Default::default()
    };
    state
        .cast
        .devices
        .lock()
        .unwrap()
        .insert(device.udn.clone(), device);
    state.cast.snapshot.lock().unwrap().current_device = Some(Device {
        id: "uuid:renderer".into(),
        ..Default::default()
    });
    execute(
        &state,
        &state.cast,
        Command::Cast {
            item: Item {
                path: "/video.mp4".into(),
                title: "A & B".into(),
                ..Default::default()
            },
        },
    )
    .await
    .unwrap();
    assert!(
        state.cast.snapshot().items.is_empty(),
        "Tap to cast must not enqueue"
    );
    assert_eq!(state.cast.snapshot().sid, "uuid:test-subscription");
    execute(&state, &state.cast, Command::Pause).await.unwrap();
    assert!(!state.cast.snapshot().playing);
    execute(
        &state,
        &state.cast,
        Command::Seek {
            position_ms: 3723999,
        },
    )
    .await
    .unwrap();
    execute(&state, &state.cast, Command::Exit).await.unwrap();
    assert_eq!(state.cast.snapshot().current_uri, "/video.mp4");
    assert!(state.cast.snapshot().current_device.is_some());
    assert!(state.cast.snapshot().sid.is_empty());
    let rows = calls.lock().unwrap().clone();
    assert!(rows[0].1.contains("SetAVTransportURI"));
    assert!(rows[0].2.contains("A &amp;amp; B"));
    assert!(rows[1].1.contains("#Play"));
    assert_eq!(rows[2].0, "SUBSCRIBE");
    assert!(
        rows.iter()
            .any(|row| row.1.contains("#Seek") && row.2.contains("01:02:03"))
    );
    assert!(
        rows.iter()
            .any(|row| row.0 == "UNSUBSCRIBE" && row.1 == "uuid:test-subscription")
    );
    assert!(
        !rows.iter().any(|row| row.1.contains("#Stop")),
        "Exit retains receiver playback"
    );
    state.prefs.set_user("service", true).unwrap();
    state.cast.snapshot.lock().unwrap().items = vec![
        Item {
            path: "/video.mp4".into(),
            ..Default::default()
        },
        Item {
            path: "/next.mp4".into(),
            title: "Next".into(),
            ..Default::default()
        },
    ];
    let notify = |sequence: u32, sid: Option<&str>| {
        let mut builder = axum::http::Request::builder()
            .method("NOTIFY")
            .header("SEQ", sequence.to_string());
        if let Some(sid) = sid {
            builder = builder.header("SID", sid);
        }
        builder.body(axum::body::Body::from(r#"<Event><InstanceID val="0"><TransportState val="STOPPED"/></InstanceID></Event>"#)).unwrap()
    };
    let callback = super::super::cast_runtime::callback;
    let before = calls.lock().unwrap().len();
    callback(axum::extract::State(state.clone()), notify(10, None)).await;
    assert_eq!(state.cast.snapshot().current_uri, "/next.mp4");
    assert_eq!(
        calls.lock().unwrap().len(),
        before + 1,
        "Track advance sets URI without explicit Play"
    );
    callback(axum::extract::State(state.clone()), notify(10, None)).await;
    callback(axum::extract::State(state.clone()), notify(9, None)).await;
    callback(
        axum::extract::State(state.clone()),
        notify(11, Some("uuid:foreign")),
    )
    .await;
    assert_eq!(
        state.cast.snapshot().current_uri,
        "/next.mp4",
        "Duplicate, stale and foreign notifications must not advance"
    );
    callback(axum::extract::State(state.clone()), notify(11, None)).await;
    assert_eq!(
        state.cast.snapshot().current_uri,
        "/video.mp4",
        "Cast queue wraps"
    );
    execute(&state, &state.cast, Command::Stop).await.unwrap();
    assert!(state.cast.snapshot().current_device.is_none());
    assert!(state.cast.snapshot().current_uri.is_empty());
    assert!(
        calls
            .lock()
            .unwrap()
            .iter()
            .any(|row| row.1.contains("#Stop"))
    );
    server.shutdown().await;
    host_task.abort();
    renderer.abort();
}
