use super::*;
#[cfg(feature = "http_transport")]
const TOKEN: &str = "AwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwM=";

#[cfg(feature = "http_transport")]
fn fixture() -> (tempfile::TempDir, super::super::ContentServer) {
    let dir = tempfile::tempdir().unwrap();
    let prefs =
        std::sync::Arc::new(crate::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    let (key, public) = crate::ed25519_generate();
    prefs.set("client_id", "actor").unwrap();
    prefs
        .set(
            "signature_key_pair",
            json!({
                "privateKey": crate::base64_encode(&key[..32]),
                "publicKey": crate::base64_encode(&public),
            })
            .to_string(),
        )
        .unwrap();
    let server = super::super::ContentServer::start(
        &dir.path().join("plain.db"),
        &crate::base64_encode(&[3; 32]),
        prefs,
    )
    .unwrap();
    let host=server.runtime_state().host.clone();
    let (generation,mut requests)=host.connect();
    tokio::spawn(async move {
        while let Some(request)=requests.recv().await {
            let result=match request["method"].as_str().unwrap() {
                "mdnsMulticast"=>json!(true),
                "systemCastAddressFacts"=>json!({"deviceName":"Test Phone"}),
                other=>panic!("Unexpected platform primitive: {other}"),
            };
            let _=host.reply(generation,json!({"id":request["id"],"result":result}));
        }
    });
    assert_eq!(
        TOKEN,
        crate::base64_encode(&[3; 32]),
        "token literal drifted"
    );
    (dir, server)
}

/// The receiver endpoints live on the public web listener, not the private
/// content port, so the public surface has to be started explicitly.
#[cfg(feature = "http_transport")]
async fn public_fixture() -> (tempfile::TempDir, super::super::ContentServer, u16) {
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
    let state = crate::dlna_receiver::renderer_state::DlnaRendererState::default();
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
    crate::prefs::dlna::add_sender(&state.prefs, "dlna_allowed_senders", "10.0.0.9", "TV");

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
async fn host_control_surfaces_the_lifecycle_and_the_cast_prompt() {
    let (dir, server) = fixture();
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/dlna/receiver", server.port);
    let state = server.runtime_state();
    let token = TOKEN.to_string();
    let post = |body: Value| {
        let client = client.clone();
        let url = url.clone();
        let token = token.clone();
        async move {
            let response = client
                .post(&url)
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(body.to_string())
                .send()
                .await
                .unwrap();
            (response.status(), response.text().await.unwrap())
        }
    };

    let anonymous = client
        .post(&url)
        .header("content-type", "application/json")
        .body(json!({"action": "start", "port": 7878}).to_string())
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        anonymous, 401,
        "the host control surface must require the local token"
    );

    let (status, body) = post(json!({"action": "start", "port": 7878})).await;
    assert_eq!(status, 200);
    let row: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(row["result"]["isRunning"], json!(true));
    assert_eq!(row["result"]["port"], json!(7878));

    // Seed a pending request, then let the host accept it and remember the
    // sender — the same two-step the cast dialog performs.
    state.dlna.state.write().await.pending_cast_request =
        Some(crate::dlna_receiver::types::PendingCastRequest {
            sender_ip: "10.0.0.9".into(),
            sender_name: "TV".into(),
            media_uri: "http://example/a.mp4".into(),
            media_title: "a".into(),
            media_type: crate::dlna_receiver::types::DlnaMediaType::VIDEO,
            album_art_uri: String::new(),
        });

    let (status, body) = post(json!({"action": "accept", "remember": true})).await;
    assert_eq!(status, 200);
    let row: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(row["result"]["pendingCastRequest"], Value::Null);
    assert_eq!(row["result"]["mediaUri"], json!("http://example/a.mp4"));
    assert!(crate::prefs::dlna::senders_contain_ip(
        &crate::prefs::dlna::senders(&state.prefs, "dlna_allowed_senders"),
        "10.0.0.9"
    ));

    // A rejected request lands the opposite way.
    state.dlna.state.write().await.pending_cast_request =
        Some(crate::dlna_receiver::types::PendingCastRequest {
            sender_ip: "10.0.0.8".into(),
            sender_name: "Other".into(),
            media_uri: "http://example/b.mp4".into(),
            media_title: "b".into(),
            media_type: crate::dlna_receiver::types::DlnaMediaType::AUDIO,
            album_art_uri: String::new(),
        });
    let (status, body) = post(json!({"action": "reject", "remember": true})).await;
    assert_eq!(status, 200);
    let row: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(row["result"]["pendingCastRequest"], Value::Null);
    assert_ne!(row["result"]["mediaUri"], json!("http://example/b.mp4"));
    assert!(crate::prefs::dlna::senders_contain_ip(
        &crate::prefs::dlna::senders(&state.prefs, "dlna_denied_senders"),
        "10.0.0.8"
    ));

    let (status, body) =
        post(json!({"action": "position", "positionMs": 4200, "durationMs": 9000})).await;
    assert_eq!(status, 200, "position failed: {body}");
    let row: Value = serde_json::from_str(&body).unwrap_or_else(|e| panic!("{e}: {body}"));
    assert_eq!(row["result"]["currentPositionMs"], json!(4200));
    assert_eq!(row["result"]["durationMs"], json!(9000));

    let (_, body) = post(json!({"action": "stop"})).await;
    let row: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(row["result"]["isRunning"], json!(false));
    drop(dir);
}
