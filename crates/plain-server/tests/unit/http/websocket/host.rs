use super::*;
#[test]
fn upstream_json_keeps_the_control_envelope_registry() {
    let input=controls(br#"{"type":"screenMirrorControl","input":{"action":"SCROLL","x":0.5,"y":0.25,"deltaX":-10,"deltaY":120}}"#);
    assert_eq!(input.len(), 1);
    assert_eq!(input[0]["action"], "SCROLL");
    assert_eq!(input[0]["deltaX"], -10);
    for bytes in [
        br#"{"type":"deviceCommand","action":"BACK"}"#.as_slice(),
        br#"{"action":"BACK"}"#,
        b"not json",
        b"{}",
        b"[1,2,3]",
        br#"{"type":"screenMirrorControl","input":{"action":"INVALID"}}"#,
    ] {
        assert!(controls(bytes).is_empty());
    }
}
#[test]
fn touch_frames_keep_endianness_order_cancel_and_the_reserved_stream_id() {
    let bytes = [
        0x54, 4, 0xff, 0xff, 0, 7, 0x34, 0x12, 0x78, 0x56, 0, 0, 1, 3, 0xff, 0xff, 0xff, 0xff, 0,
        0, 2, 7, 0, 0, 0, 0, 0, 0, 3, 7, 0, 0, 0, 0, 0, 0,
    ];
    let items = controls(&bytes);
    assert_eq!(items.len(), 4);
    assert_eq!(
        items
            .iter()
            .map(|row| row["action"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["TOUCH_DOWN", "TOUCH_MOVE", "TOUCH_UP", "TOUCH_UP"]
    );
    assert_eq!(items[0]["pointerId"], 7);
    assert_eq!(
        items[0]["x"].as_f64().unwrap(),
        (f32::from(0x1234u16) / 65535.0) as f64
    );
    assert_eq!(
        items[0]["y"].as_f64().unwrap(),
        (f32::from(0x5678u16) / 65535.0) as f64
    );
    assert_eq!(items[1]["x"], 1.0);
    for bytes in [
        b"".as_slice(),
        &[0x54],
        &[0x54, 0, 0, 0],
        &[0x53, 1, 0, 0],
        &[0x54, 1, 0, 0, 0],
    ] {
        assert!(controls(bytes).is_empty());
    }
}
#[test]
fn rejecting_confirmation_retains_the_legacy_retry_close_code() {
    let runtime = Runtime::default();
    let (outgoing, _) = mpsc::channel(1);
    let (closing, receiver) = tokio::sync::watch::channel(None);
    runtime.connections.lock().unwrap().insert(
        "connection".into(),
        Connection {
            client_id: "client".into(),
            registered: false,
            pending: Some("request".into()),
            outgoing,
            closing,
        },
    );
    runtime.reject("request");
    assert_eq!(*receiver.borrow(), Some((1013, "rejected")));
}

#[tokio::test]
async fn auth_confirmation_uses_native_events_and_returns_the_hosts_decision_to_the_browser() {
    use futures_util::SinkExt;
    use sha2::Digest;
    use tokio_tungstenite::tungstenite::Message as ClientMessage;
    let dir = tempfile::tempdir().unwrap();
    let prefs =
        std::sync::Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap());
    prefs.set_user("service", true).unwrap();
    prefs.set_user("desktop_access", true).unwrap();
    prefs.set("auth_two_factor", true).unwrap();
    prefs.set("password", "secret").unwrap();
    prefs.set("client_id", "phone").unwrap();
    let (private, _) = plain_rs::crypto::ed25519_generate();
    prefs
        .set(
            "signature_key_pair",
            plain_rs::utils::base64::base64_encode(&private),
        )
        .unwrap();
    let server = crate::assembly::server::ContentServer::start(
        &dir.path().join("db"),
        &plain_rs::utils::base64::base64_encode(&[3; 32]),
        prefs,
    )
    .unwrap();
    let bridge = std::sync::Arc::new(server.host_events());
    let host = bridge.clone();
    let consumer = tokio::spawn(async move {
        while let Some(request) = host.next_request().await {
            assert_ne!(request["method"], "systemWebLoginRequest");
            host.reply(json!({"id":request["id"],"result":null}))
                .unwrap();
        }
    });
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let (port, _) = server
        .start_public(
            0,
            0,
            cert.cert.pem().into_bytes(),
            cert.key_pair.serialize_pem().into_bytes(),
        )
        .await
        .unwrap();
    let digest = plain_rs::utils::hex::bytes_to_hex(&sha2::Sha512::digest(b"secret"));
    let client = plain_rs::crypto::EcdhSession::generate();
    let request = json!({"password":digest,"browserName":"Test","browserVersion":"1","osName":"Fixture","osVersion":"1","isMobile":false,"ecdhPublicKey":plain_rs::utils::base64::base64_encode(&client.public_key_bytes),"peer":null});
    let frame = plain_rs::crypto::xchacha_encrypt_raw(
        &digest.as_bytes()[..32],
        &serde_json::to_vec(&request).unwrap(),
    )
    .unwrap();
    for (cid, allowed) in [("rejected-browser", false), ("accepted-browser", true)] {
        let (mut socket, _) =
            tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/?cid={cid}&auth=1"))
                .await
                .unwrap();
        socket
            .send(ClientMessage::Binary(frame.clone().into()))
            .await
            .unwrap();
        let response = tokio::time::timeout(Duration::from_secs(3), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(matches!(response, ClientMessage::Binary(_)));
        let event = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let event = bridge.next_event().await.unwrap();
                if event["type"] == "WEB_LOGIN_REQUESTED" {
                    break event;
                }
            }
        })
        .await
        .unwrap();
        let payload: Value = serde_json::from_str(event["payload"].as_str().unwrap()).unwrap();
        assert_eq!(payload["clientId"], cid);
        assert_eq!(payload["request"]["password"], "");
        assert_eq!(payload["request"]["browserName"], "Test");
        assert!(
            plain_rs::events::WsEvent::broadcast("WEB_LOGIN_REQUESTED", String::new())
                .is_internal()
        );
        let id = payload["requestId"].as_str().unwrap();
        server
            .web_login_confirmation()
            .decide(id, allowed)
            .await
            .unwrap();
        let response = tokio::time::timeout(Duration::from_secs(3), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        if allowed {
            let ClientMessage::Binary(frame) = response else {
                panic!("expected completed auth frame")
            };
            let decrypted =
                plain_rs::crypto::xchacha_decrypt_raw(&digest.as_bytes()[..32], &frame).unwrap();
            let result: Value = serde_json::from_slice(&decrypted).unwrap();
            assert_eq!(result["status"], "COMPLETED");
            assert!(
                server
                    .runtime_state()
                    .db
                    .session_get(cid)
                    .unwrap()
                    .is_some()
            );
        } else {
            let ClientMessage::Close(Some(frame)) = response else {
                panic!("expected rejected close frame")
            };
            assert_eq!(u16::from(frame.code), 1013);
            assert_eq!(frame.reason, "rejected");
            assert!(
                server
                    .runtime_state()
                    .db
                    .session_get(cid)
                    .unwrap()
                    .is_none()
            );
        }
        assert!(
            server
                .web_login_confirmation()
                .decide(id, true)
                .await
                .is_err()
        );
    }
    server.shutdown().await;
    consumer.await.unwrap();
}
