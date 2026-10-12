use super::*;
#[cfg(feature = "http_transport")]
const TOKEN: &str = "AwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwM=";

#[cfg(feature = "http_transport")]
fn fixture() -> (tempfile::TempDir, crate::assembly::server::ContentServer) {
    let dir = tempfile::tempdir().unwrap();
    let prefs =
        std::sync::Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    let (key, public) = plain_rs::crypto::ed25519_generate();
    prefs.set("client_id", "actor").unwrap();
    prefs
        .set(
            "signature_key_pair",
            plain_rs::utils::base64::base64_encode(&key),
        )
        .unwrap();
    let server = crate::assembly::server::ContentServer::start(
        &dir.path().join("plain.db"),
        &plain_rs::utils::base64::base64_encode(&[3; 32]),
        prefs,
    )
    .unwrap();
    let host = server.runtime_state().host.clone();
    let (generation, mut requests) = host.connect();
    tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let result = match request["method"].as_str().unwrap() {
                "mdnsMulticast" => json!(true),
                "systemCastAddressFacts" => {
                    json!({"ip":"127.0.0.1","defaultDeviceName":"Test Phone"})
                }
                other => panic!("Unexpected platform primitive: {other}"),
            };
            let _ = host.reply(generation, json!({"id":request["id"],"result":result}));
        }
    });
    assert_eq!(
        TOKEN,
        plain_rs::utils::base64::base64_encode(&[3; 32]),
        "token literal drifted"
    );
    (dir, server)
}

/// The receiver endpoints live on the public web listener, not the private
/// content port, so the public surface has to be started explicitly.
#[cfg(feature = "http_transport")]
async fn public_fixture() -> (
    tempfile::TempDir,
    crate::assembly::server::ContentServer,
    u16,
) {
    let (dir, server) = fixture();
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let (http, _) = server
        .start_public(
            0,
            0,
            cert.cert.pem().into_bytes(),
            cert.key_pair.serialize_pem().into_bytes(),
        )
        .await
        .unwrap();
    (dir, server, http)
}

#[test]
fn snapshot_omits_the_internal_pending_slot() {
    let state = crate::services::casting::receiver::renderer_state::DlnaRendererState::default();
    let row = snapshot(&state);
    // The UI only ever needs the confirmed request; `raw_pending_cast_request`
    // is the rules input and must not leak into the host surface.
    assert!(row.get("rawPendingCastRequest").is_none());
    assert_eq!(row["pendingCastRequest"], Value::Null);
    assert_eq!(row["playbackState"], "NoMediaPresent");
    assert_eq!(row["seekTargetMs"], Value::Null);
}

#[test]
fn local_ip_skips_loopback_so_it_matches_the_ssdp_location() {
    // A loopback-only host still has to produce something usable rather than
    // an empty string in the device description.
    let ip = local_ip();
    assert!(!ip.is_empty());
    assert!(!ip.starts_with("127."), "picked {ip}");
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn receiver_paths_answer_only_while_the_toggle_and_service_are_on() {
    let (dir, server, port) = public_fixture().await;
    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{port}");
    let state = server.runtime_state();

    let describe = || async {
        client
            .get(format!("{base}/description.xml"))
            .send()
            .await
            .unwrap()
            .status()
    };

    // Both prefs off, which is the default: the renderer must not be
    // discoverable or answerable at all.
    assert_eq!(describe().await, 404);

    state.prefs.set_user("service", true).unwrap();
    assert_eq!(describe().await, 404, "service alone is not enough");

    state.prefs.set_user("dlna", true).unwrap();
    let body = client
        .get(format!("{base}/description.xml"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        body.contains("<root"),
        "expected a device description, got {body}"
    );
    assert!(body.contains(server.runtime_state().dlna.device_uuid()));
    // `ensure_identity` seeds the device-name preference from the OS during
    // core construction, so the receiver answers with that name rather than
    // the host `defaultDeviceName`; only presence is contractual here.
    assert!(
        body.contains("<friendlyName>"),
        "expected a friendly name element, got {body}"
    );

    // The service description documents are static and served the same way.
    for path in ["/AVTransport/scpd.xml", "/RenderingControl/scpd.xml"] {
        assert_eq!(
            client
                .get(format!("{base}{path}"))
                .send()
                .await
                .unwrap()
                .status(),
            200,
            "{path}"
        );
    }
    // An unregistered path is not the receiver's business: it falls through to
    // the SPA/bridge fallback, and must not be answered as UPnP.
    let stray = client
        .get(format!("{base}/AVTransport/nope"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        !stray.contains("<root"),
        "the receiver answered an unknown path"
    );
    drop(dir);
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn a_forged_c_ip_header_cannot_impersonate_an_allowed_sender() {
    let (dir, server, port) = public_fixture().await;
    let client = reqwest::Client::new();
    let base = format!("http://127.0.0.1:{port}");
    let state = server.runtime_state();
    state.prefs.set_user("service", true).unwrap();
    state.prefs.set_user("dlna", true).unwrap();
    // 10.0.0.9 is "trusted"; the request below actually arrives from loopback.
    plain_rs::prefs::dlna::add_sender(&state.prefs, "dlna_allowed_senders", "10.0.0.9", "TV");

    let soap = r#"<?xml version="1.0"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body><u:SetAVTransportURI xmlns:u="urn:schemas-upnp-org:service:AVTransport:1"><InstanceID>0</InstanceID><CurrentURI>http://example/a.mp4</CurrentURI><CurrentURIMetaData/></u:SetAVTransportURI></s:Body></s:Envelope>"#;
    let status = client
        .post(format!("{base}/AVTransport/control"))
        .header("content-type", "text/xml")
        .header(
            "soapaction",
            r#""urn:schemas-upnp-org:service:AVTransport:1#SetAVTransportURI""#,
        )
        .header("c-ip", "10.0.0.9")
        .body(soap)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status, 200);

    // The trusted IP never spoke to us, so the request must not have been
    // auto-accepted — and what we recorded has to be the socket peer, not the
    // header the control point chose.
    let after = state.dlna.snapshot().await;
    let pending = after
        .pending_cast_request
        .expect("the request should be waiting for the user");
    assert_eq!(pending.sender_ip, "127.0.0.1");
    assert_eq!(pending.sender_name, "127.0.0.1");
    assert!(
        after.media_uri.is_empty(),
        "a forged c-ip header was trusted: {}",
        after.media_uri
    );
    drop(dir);
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn native_control_surfaces_lifecycle_and_cast_prompt_without_public_http() {
    let (dir, server, _) = public_fixture().await;
    server.stop_public().await;
    let controller = server.dlna_receiver();
    let state = server.runtime_state();
    let post = |body: Value| {
        let controller = &controller;
        async move {
            let value: Value =
                serde_json::from_str(&controller.execute_json(&body.to_string()).await.unwrap())
                    .unwrap();
            (200, json!({"result": value}).to_string())
        }
    };

    assert!(
        controller
            .execute_json(r#"{"action":"position","positionMs":1}"#)
            .await
            .is_err()
    );
    assert!(
        controller
            .execute_json(r#"{"action":"snapshot","unexpected":true}"#)
            .await
            .is_err()
    );
    let (status, body) = post(json!({"action": "start", "port": 7878})).await;
    assert_eq!(status, 200);
    let row: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(row["result"]["isRunning"], json!(true));
    assert_eq!(row["result"]["port"], json!(7878));

    // Seed a pending request, then let the host accept it and remember the
    // sender — the same two-step the cast dialog performs.
    state.dlna.state.write().await.pending_cast_request = Some(
        crate::services::casting::receiver::types::PendingCastRequest {
            sender_ip: "10.0.0.9".into(),
            sender_name: "TV".into(),
            media_uri: "http://example/a.mp4".into(),
            media_title: "a".into(),
            media_type: crate::services::casting::receiver::types::DlnaMediaType::VIDEO,
            album_art_uri: String::new(),
        },
    );

    let (status, body) = post(json!({"action": "accept", "remember": true})).await;
    assert_eq!(status, 200);
    let row: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(row["result"]["pendingCastRequest"], Value::Null);
    assert_eq!(row["result"]["mediaUri"], json!("http://example/a.mp4"));
    assert!(plain_rs::prefs::dlna::senders_contain_ip(
        &plain_rs::prefs::dlna::senders(&state.prefs, "dlna_allowed_senders"),
        "10.0.0.9"
    ));

    // A rejected request lands the opposite way.
    state.dlna.state.write().await.pending_cast_request = Some(
        crate::services::casting::receiver::types::PendingCastRequest {
            sender_ip: "10.0.0.8".into(),
            sender_name: "Other".into(),
            media_uri: "http://example/b.mp4".into(),
            media_title: "b".into(),
            media_type: crate::services::casting::receiver::types::DlnaMediaType::AUDIO,
            album_art_uri: String::new(),
        },
    );
    let (status, body) = post(json!({"action": "reject", "remember": true})).await;
    assert_eq!(status, 200);
    let row: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(row["result"]["pendingCastRequest"], Value::Null);
    assert_ne!(row["result"]["mediaUri"], json!("http://example/b.mp4"));
    assert!(plain_rs::prefs::dlna::senders_contain_ip(
        &plain_rs::prefs::dlna::senders(&state.prefs, "dlna_denied_senders"),
        "10.0.0.8"
    ));

    let (status, body) =
        post(json!({"action": "position", "positionMs": 4200, "durationMs": 9000})).await;
    assert_eq!(status, 200, "position failed: {body}");
    let row: Value = serde_json::from_str(&body).unwrap_or_else(|e| panic!("{e}: {body}"));
    assert_eq!(row["result"]["currentPositionMs"], json!(4200));
    assert_eq!(row["result"]["durationMs"], json!(9000));

    let rules: Value = serde_json::from_str(
        &controller
            .execute_json(r#"{"action":"rules"}"#)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(rules["allowed"][0]["ip"], "10.0.0.9");
    let rules: Value = serde_json::from_str(
        &controller
            .execute_json(r#"{"action":"removeRule","ip":"10.0.0.9","allowed":true}"#)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(rules["allowed"], json!([]));
    post(json!({"action":"playback","state":"PausedPlayback"})).await;
    state.dlna.state.write().await.seek_target_ms = Some(3000);
    post(json!({"action":"seekConsumed"})).await;
    let snapshot: Value = serde_json::from_str(
        &controller
            .execute_json(r#"{"action":"snapshot"}"#)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(snapshot["playbackState"], "PausedPlayback");
    assert_eq!(snapshot["seekTargetMs"], Value::Null);
    post(json!({"action":"stopped"})).await;
    let snapshot: Value = serde_json::from_str(
        &controller
            .execute_json(r#"{"action":"snapshot"}"#)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(snapshot["mediaUri"], "");
    post(json!({"action":"retry","port":7878})).await;
    let (_, body) = post(json!({"action": "stop"})).await;
    let row: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(row["result"]["isRunning"], json!(false));
    drop(dir);
}

#[tokio::test]
async fn public_dlna_reads_the_native_players_reported_position() {
    let (_dir, server, port) = public_fixture().await;
    let state = server.runtime_state();
    state.prefs.set_user("service", true).unwrap();
    state.prefs.set_user("dlna", true).unwrap();
    let controller = server.dlna_receiver();
    controller
        .execute_json(r#"{"action":"position","positionMs":15000,"durationMs":120000}"#)
        .await
        .unwrap();
    let soap = r#"<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body><u:GetPositionInfo xmlns:u="urn:schemas-upnp-org:service:AVTransport:1"><InstanceID>0</InstanceID></u:GetPositionInfo></s:Body></s:Envelope>"#;
    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/AVTransport/control"))
        .header(
            "soapaction",
            r#""urn:schemas-upnp-org:service:AVTransport:1#GetPositionInfo""#,
        )
        .body(soap)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    assert!(body.contains("<RelTime>00:00:15</RelTime>"), "{body}");
    assert!(
        body.contains("<TrackDuration>00:02:00</TrackDuration>"),
        "{body}"
    );
}

#[tokio::test]
async fn a_body_at_the_cap_is_read_and_one_byte_over_is_not() {
    let at_cap =
        axum::body::Body::from_stream(futures_util::stream::iter(vec![Ok::<_, std::io::Error>(
            vec![0u8; MAX_BODY],
        )]));
    assert_eq!(read_body(at_cap, MAX_BODY).await.unwrap().len(), MAX_BODY);

    let over =
        axum::body::Body::from_stream(futures_util::stream::iter(vec![Ok::<_, std::io::Error>(
            vec![0u8; MAX_BODY + 1],
        )]));
    assert_eq!(read_body(over, MAX_BODY).await, Err(BodyFailure::TooLarge));
}

#[tokio::test]
async fn a_body_that_fails_mid_read_is_reported_as_unreadable() {
    let stream = futures_util::stream::iter(vec![
        Ok(vec![1u8, 2, 3]),
        Err(std::io::Error::other("peer reset")),
    ]);
    assert_eq!(
        read_body(axum::body::Body::from_stream(stream), MAX_BODY).await,
        Err(BodyFailure::Unreadable)
    );
}

#[tokio::test]
async fn the_two_body_faults_answer_differently() {
    assert_eq!(
        StatusCode::from(BodyFailure::TooLarge),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(
        StatusCode::from(BodyFailure::Unreadable),
        StatusCode::BAD_REQUEST
    );
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn an_oversized_control_request_never_reaches_the_protocol() {
    let (dir, server, port) = public_fixture().await;
    let state = server.runtime_state();
    state.prefs.set_user("service", true).unwrap();
    state.prefs.set_user("dlna", true).unwrap();

    let soap = format!(
        r#"<?xml version="1.0"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body><u:SetAVTransportURI xmlns:u="urn:schemas-upnp-org:service:AVTransport:1"><InstanceID>0</InstanceID><CurrentURI>http://example/{}</CurrentURI><CurrentURIMetaData/></u:SetAVTransportURI></s:Body></s:Envelope>"#,
        "a".repeat(MAX_BODY)
    );
    assert!(soap.len() > MAX_BODY, "the request must exceed the cap");

    let status = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/AVTransport/control"))
        .header("content-type", "text/xml")
        .header(
            "soapaction",
            r#""urn:schemas-upnp-org:service:AVTransport:1#SetAVTransportURI""#,
        )
        .body(soap)
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);

    // Before the cap was enforced the same request arrived as an empty SOAP
    // body, which the router answered and the command channel carried.
    let after = state.dlna.snapshot().await;
    assert!(after.pending_cast_request.is_none());
    assert!(after.media_uri.is_empty());
    drop(dir);
}

#[tokio::test]
async fn receiver_name_uses_platform_default_and_core_setting() {
    let (_dir, server) = fixture();
    let state = server.runtime_state();
    state.prefs.set_user("device_name", "").unwrap();
    assert_eq!(device_name(&state).await.unwrap(), "Test Phone");
    state
        .prefs
        .set_user("device_name", "Shared Renderer")
        .unwrap();
    assert_eq!(device_name(&state).await.unwrap(), "Shared Renderer");
    let (_generation, _requests) = state.host.connect();
    assert_eq!(device_name(&state).await.unwrap(), "Shared Renderer");
}

#[tokio::test]
async fn receiver_name_rejects_missing_platform_name() {
    let (_dir, server) = fixture();
    let state = server.runtime_state();
    state.prefs.set_user("device_name", "").unwrap();
    let host = state.host.clone();
    let (generation, mut requests) = host.connect();
    let reply = tokio::spawn(async move {
        let request = requests.recv().await.unwrap();
        host.reply(
            generation,
            json!({"id":request["id"],"result":{"ip":"127.0.0.1"}}),
        )
        .unwrap();
    });
    assert!(device_name(&state).await.is_err());
    reply.await.unwrap();
}
