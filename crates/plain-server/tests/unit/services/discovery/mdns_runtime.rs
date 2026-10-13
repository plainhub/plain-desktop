use super::*;

#[tokio::test]
async fn authenticated_runtime_preserves_radio_modes_and_releases_global_owner() {
    use std::sync::atomic::AtomicBool;
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    prefs.set("client_id", "fixture-mdns-root").unwrap();
    prefs
        .set("mdns_hostname", "fixture-mdns-root.local")
        .unwrap();
    prefs.set_user("https_port", 2443).unwrap();
    let token = plain_rs::utils::base64::base64_encode(&[5; 32]);
    let server = crate::assembly::server::ContentServer::start(
        &dir.path().join("plain.db"),
        &token,
        prefs.clone(),
    )
    .unwrap();
    let state = server.runtime_state();
    // The advertised port is the live public listener's, so the test has to
    // start one — announcing port 0 is refused by contract.
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    server
        .start_public(
            0,
            0,
            cert.cert.pem().into_bytes(),
            cert.key_pair.serialize_pem().into_bytes(),
        )
        .await
        .unwrap();
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
    // The desktop reaches mDNS through the native controller, not an HTTP
    // route: the public router never had one and `/chat/mdns` only exists on
    // the legacy listener this test no longer drives.
    let discovery = crate::native::discovery::NativeDiscovery::new(state.clone());
    allowed.store(true, Ordering::SeqCst);
    state
        .nearby_devices
        .scanning_modes(None, Some(true), vec![])
        .unwrap();
    let start: Value = execute(&state, Request::Start {}).await;
    assert_eq!(start["receiver"], true);
    assert_eq!(start["scanning"], true);
    assert!(acquired.load(Ordering::SeqCst));
    allowed.store(false, Ordering::SeqCst);
    let stop: Value = execute(&state, Request::Stop {}).await;
    assert_eq!(stop["scanning"], false);
    assert!(
        state.nearby_devices.active(),
        "Stopping LAN must preserve active BLE"
    );
    state
        .nearby_devices
        .scanning_modes(None, Some(false), vec![])
        .unwrap();
    assert!(!state.nearby_devices.active());
    allowed.store(true, Ordering::SeqCst);
    let published: Value = execute(&state, Request::Publish {}).await;
    assert_eq!(published["published"], true);
    prefs.set("mdns_hostname", "fixture-new.local").unwrap();
    let updated: Value = execute(&state, Request::Update {}).await;
    assert_eq!(updated["published"], true);
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
    allowed.store(false, Ordering::SeqCst);
    let withdrawn: Value = execute(&state, Request::Unpublish {}).await;
    assert_eq!(withdrawn["published"], false);
    let updated: Value = execute(&state, Request::Update {}).await;
    assert_eq!(
        updated["published"], false,
        "Update cannot republish an inactive service"
    );
    allowed.store(true, Ordering::SeqCst);
    let debug_a = uuid::Uuid::new_v4();
    let debug_b = uuid::Uuid::new_v4();
    let token_string = token.clone();
    let debug = execute(
        &state,
        Request::DebugStart {
            token: debug_a.to_string(),
        },
    )
    .await;
    assert_eq!(debug["scanning"], true);
    assert_eq!(debug["capturing"], true);
    let sender = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let receiver = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    receiver
        .set_read_timeout(Some(std::time::Duration::from_secs(1)))
        .unwrap();
    let bytes = plain_rs::protocols::mdns::packet_codec::build_ptr_query("_plainapp._tcp.local");
    plain_rs::protocols::mdns::host_responder::send_recorded(
        &sender,
        &bytes,
        receiver.local_addr().unwrap(),
    )
    .unwrap();
    let mut buffer = [0; 1500];
    let (size, src) = receiver.recv_from(&mut buffer).unwrap();
    assert_eq!(&buffer[..size], bytes.as_slice());
    plain_rs::protocols::mdns::host_responder::record_received(&receiver, src, &buffer[..size]);
    let captured = state.mdns.snapshot();
    assert!(
        captured["packetsIn"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["srcPort"] == sender.local_addr().unwrap().port()
                && p["summary"].as_str().unwrap().contains("PTR"))
    );
    assert!(captured["packetsOut"].as_array().unwrap().iter().any(|p| {
        p["dstPort"] == receiver.local_addr().unwrap().port()
            && p["detail"]
                .as_str()
                .unwrap()
                .contains("_plainapp._tcp.local")
    }));
    execute(
        &state,
        Request::DebugStart {
            token: debug_b.to_string(),
        },
    )
    .await;
    execute(&state, Request::Stop {}).await;
    assert_eq!(
        state.mdns.snapshot()["scanning"],
        true,
        "Debug leases must keep scanning independently of manual stop"
    );
    execute(
        &state,
        Request::DebugStop {
            token: debug_a.to_string(),
        },
    )
    .await;
    assert_eq!(state.mdns.snapshot()["scanning"], true);
    execute(
        &state,
        Request::DebugStop {
            token: debug_b.to_string(),
        },
    )
    .await;
    assert_eq!(state.mdns.snapshot()["scanning"], false);
    assert_eq!(state.mdns.snapshot()["capturing"], false);
    assert_eq!(state.mdns.snapshot()["packetsIn"], json!([]));
    assert_eq!(state.mdns.snapshot()["packetsOut"], json!([]));
    execute(&state, Request::Start {}).await;
    execute(
        &state,
        Request::DebugStart {
            token: debug_a.to_string(),
        },
    )
    .await;
    execute(
        &state,
        Request::DebugStop {
            token: debug_a.to_string(),
        },
    )
    .await;
    assert_eq!(
        state.mdns.snapshot()["scanning"],
        true,
        "Debug page disposal must preserve manual discovery"
    );
    execute(&state, Request::Stop {}).await;
    state.host.disconnect(generation);
    responder.abort();
    acquired.store(false, Ordering::SeqCst);
    Runtime::host_connected(state.clone(), generation).await;
    assert!(
        !acquired.load(Ordering::SeqCst),
        "An old host generation must not reacquire permission"
    );
    let (generation, mut rx) = state.host.connect();
    let host = state.host.clone();
    let lease = acquired.clone();
    let responder = tokio::spawn(async move {
        while let Some(request) = rx.recv().await {
            assert_eq!(request["method"], "mdnsMulticast");
            lease.store(
                request["params"]["acquire"].as_bool().unwrap(),
                Ordering::SeqCst,
            );
            host.reply(generation, json!({"id":request["id"],"result":true}))
                .unwrap();
        }
    });
    Runtime::host_connected(state.clone(), generation).await;
    assert!(
        acquired.load(Ordering::SeqCst),
        "The current host must restore the actual multicast lease"
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

async fn execute(state: &ServerState, request: Request) -> Value {
    state
        .mdns
        .execute(state, request)
        .await
        .unwrap_or_else(|error| panic!("mDNS request failed: {error}"))
}
