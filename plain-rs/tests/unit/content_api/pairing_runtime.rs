use super::*;
#[test]
fn ble_associations_match_signature_and_cleanup_without_consuming_replacements() {
    let runtime = Runtime::default();
    let mut request:PairingRequest=serde_json::from_value(json!({"fromId":"peer","fromName":"fixture","port":2443,"deviceType":"PHONE","ecdhPublicKey":"","signaturePublicKey":"","timestamp":1,"ips":[],"signature":"first","awareSupported":false})).unwrap();
    runtime.remember(&request, "mac-first").unwrap();
    request.signature = "second".into();
    runtime.remember(&request, "mac-second").unwrap();
    assert!(runtime.take("peer", "first").is_none());
    assert_eq!(runtime.take("peer", "second").unwrap(), "mac-second");
    assert!(runtime.take("peer", "second").is_none());
    runtime.remember(&request, "mac-second").unwrap();
    runtime.forget("peer");
    assert!(runtime.take("peer", "second").is_none());
}

#[cfg(feature = "http_transport")]
fn fixture(id: &str) -> (tempfile::TempDir, super::super::ContentServer) {
    use std::sync::Arc;
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    let (key, public) = crate::ed25519_generate();
    prefs.set("client_id", id).unwrap();
    prefs.set("signature_key_pair",json!({"privateKey":crate::base64_encode(&key[..32]),"publicKey":crate::base64_encode(&public)}).to_string()).unwrap();
    let server = super::super::ContentServer::start(
        &dir.path().join("plain.db"),
        &crate::base64_encode(&[3; 32]),
        prefs,
    )
    .unwrap();
    (dir, server)
}
#[cfg(feature = "http_transport")]
fn device() -> Device {
    Device {
        name: "Fixture".into(),
        port: 2443,
        device_type: "PHONE".into(),
        ips: vec!["127.0.0.1".into()],
        aware_supported: false,
    }
}
#[cfg(feature = "http_transport")]
#[tokio::test]
async fn lan_runtime_sends_signed_tls_requests_and_cancel_cannot_remove_replacement() {
    use axum::{Router, body::Bytes, extract::State, routing::post};
    async fn receive(
        State(sender): State<tokio::sync::mpsc::Sender<Message>>,
        body: Bytes,
    ) -> &'static str {
        let message = Message::parse(std::str::from_utf8(&body).unwrap()).unwrap();
        sender.send(message).await.unwrap();
        "1"
    }
    let (_dir, server) = fixture("local");
    let state = server.runtime_state();
    let (send, mut received) = tokio::sync::mpsc::channel(8);
    let router = Router::new()
        .route("/nearby", post(receive))
        .with_state(send);
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let listeners = crate::http_transport::HttpListeners::start(
        router,
        0,
        0,
        cert.cert.pem().into_bytes(),
        cert.key_pair.serialize_pem().into_bytes(),
    )
    .await
    .unwrap();
    let target = Target {
        device_id: "remote".into(),
        device_name: "Fixture".into(),
        device_ip: "stale".into(),
        device_port: listeners.https_port,
    };
    let start = || Request::Start {
        methods: vec!["LAN".into(), "BLE".into()],
        ble: true,
        target: target.clone(),
        ips: vec!["invalid".into(), "127.0.0.1".into()],
        device: device(),
    };
    let first = execute(&state, start()).await.unwrap();
    assert_eq!(first["sent"], true);
    assert_eq!(first["ticket"]["deviceIp"], "127.0.0.1");
    let Message::PairRequest(request) = received.recv().await.unwrap() else {
        panic!()
    };
    assert!(crate::chat::pairing::security::verify_request(&request));
    assert_eq!(request.from_id, "local");
    let second = execute(&state, start()).await.unwrap();
    let _ = received.recv().await.unwrap();
    assert!(
        execute(
            &state,
            Request::Cancel {
                id: "remote".into(),
                generation: Some(first["ticket"]["generation"].as_str().unwrap().into())
            }
        )
        .await
        .unwrap()
        .is_null()
    );
    assert_eq!(
        state.pairing.tickets()[0].generation,
        second["ticket"]["generation"].as_str().unwrap()
    );
    let canceled = execute(
        &state,
        Request::Cancel {
            id: "remote".into(),
            generation: None,
        },
    )
    .await
    .unwrap();
    assert!(!canceled.is_null());
    let Message::PairCancel(cancel) = received.recv().await.unwrap() else {
        panic!()
    };
    assert_eq!(cancel.to_id, "remote");
    assert_eq!(cancel.from_id, "local");
    assert!(state.pairing.tickets().is_empty());
    listeners.shutdown().await;
    let failed = execute(&state, start()).await.unwrap();
    assert_eq!(failed["sent"], false);
    assert!(state.pairing.tickets().is_empty());
    server.shutdown().await;
}
#[cfg(feature = "http_transport")]
#[tokio::test]
async fn incoming_ble_response_uses_matching_address_and_shared_handshake_exactly_once() {
    let (_a, local) = fixture("local");
    let (_b, remote) = fixture("remote");
    let state = local.runtime_state();
    let remote_state = remote.runtime_state();
    let target = Target {
        device_id: "local".into(),
        device_name: "Local".into(),
        device_ip: String::new(),
        device_port: 2443,
    };
    let started =
        pairing::start(&remote_state.prefs, &remote_state.pairing, target, device()).unwrap();
    let request: PairingRequest = serde_json::from_value(started["request"].clone()).unwrap();
    assert_eq!(
        execute(
            &state,
            Request::ReceiveRequest {
                request: request.clone(),
                address: "fixture-mac".into(),
                ble: true
            }
        )
        .await
        .unwrap(),
        true
    );
    assert_eq!(
        execute(
            &state,
            Request::ReceiveRequest {
                request: request.clone(),
                address: "fixture-mac".into(),
                ble: true
            }
        )
        .await
        .unwrap(),
        false
    );
    let (generation, mut calls) = state.host.connect();
    let host = state.host.clone();
    let responder = tokio::spawn(async move {
        let call = calls.recv().await.unwrap();
        assert_eq!(call["method"], "pairingNotification");
        assert_eq!(call["params"]["address"], "fixture-mac");
        let Message::PairResponse(response) =
            Message::parse(call["params"]["body"].as_str().unwrap()).unwrap()
        else {
            panic!()
        };
        host.reply(generation, json!({"id":call["id"],"result":true}))
            .unwrap();
        response
    });
    let accepted = execute(
        &state,
        Request::Respond {
            request: request.clone(),
            accepted: true,
            device: device(),
        },
    )
    .await
    .unwrap();
    let response = responder.await.unwrap();
    let completed = pairing::complete(
        &remote_state.db,
        &remote_state.prefs,
        &remote_state.pairing,
        response,
        "",
    )
    .unwrap();
    assert_eq!(accepted["peer"]["key"], completed["peer"]["key"]);
    assert!(
        execute(
            &state,
            Request::Respond {
                request: request.clone(),
                accepted: true,
                device: device()
            }
        )
        .await
        .unwrap()
        .is_null()
    );
    assert!(
        state
            .pairing_runtime
            .take("remote", &request.signature)
            .is_none()
    );
    local.shutdown().await;
    remote.shutdown().await;
}
