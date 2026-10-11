use super::*;
#[tokio::test]
async fn peer_schema_preserves_required_arguments_and_empty_query() {
    let schema = schema();
    let sdl = schema.sdl();
    for field in [
        "createChatItem(content: String!): [ChatItem!]!",
        "channelSystemMessage(type: ChannelSystemMessageType!, payload: String!): Boolean!",
        "startAware: Boolean!",
        "createdAt: Instant!",
        "updatedAt: Instant!",
        "fromId: ID!",
        "channelId: ID",
    ] {
        assert!(sdl.contains(field), "{field}: {sdl}");
    }
    assert!(!sdl.contains("_peerSchemaVersion"));
    let result = schema.execute("mutation { createChatItem { id } }").await;
    assert!(!result.errors.is_empty());
    let result = schema
        .execute("{ __type(name: \"Query\") { fields { name } } }")
        .await;
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(
        result.data.into_json().unwrap()["__type"]["fields"],
        json!([])
    );
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn encrypted_public_and_ble_rpc_share_auth_executor_and_receipts() {
    use crate::assembly::server::ContentServer;
    use plain_rs::chat::enums::DeviceType;
    use plain_rs::chat::enums::PeerStatus;
    use plain_rs::db::DPeer;
    use plain_rs::db::Db;
    use plain_rs::db::chat_store::SaveMode;
    use plain_rs::db::chat_store::messages;
    use plain_rs::db::chat_store::peers;
    use std::sync::Arc;
    use tokio_tungstenite::tungstenite::Message;
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    prefs.set_user("service", true).unwrap();
    prefs.set_user("device_name", "fixture tablet").unwrap();
    prefs.set("client_id", "local").unwrap();
    let (local_keypair, local_public_key) = plain_rs::crypto::ed25519_generate();
    prefs
        .set(
            "signature_key_pair",
            plain_rs::utils::base64::base64_encode(&local_keypair),
        )
        .unwrap();
    let token = plain_rs::utils::base64::base64_encode(&[5; 32]);
    let db = Db::open(&dir.path().join("plain.db")).unwrap();
    let key = [7; 32];
    let (kp, public_key) = plain_rs::crypto::ed25519_generate();
    let mut peer = DPeer::new("peer", "phone", "", 443, DeviceType::Phone);
    peer.status = PeerStatus::Paired;
    peer.key = plain_rs::utils::base64::base64_encode(&key);
    peer.public_key = plain_rs::utils::base64::base64_encode(&public_key);
    peers::save(&db, &[peer], SaveMode::Insert).unwrap();
    let server = ContentServer::start(&dir.path().join("plain.db"), &token, prefs.clone()).unwrap();
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let (_, https) = server
        .start_public(
            0,
            0,
            cert.cert.pem().into_bytes(),
            cert.key_pair.serialize_pem().into_bytes(),
        )
        .await
        .unwrap();
    let mut host = crate::host::events::TestHost::new(&server);
    let (broadcast_done, broadcast_received) = tokio::sync::oneshot::channel();
    let ble = server.runtime_state().ble_transport.clone();
    let responder = tokio::spawn(async move {
        let mut broadcast_done = Some(broadcast_done);
        let mut count = 0;
        while let Some(Ok(message)) = host.next().await {
            if !message.is_text() {
                continue;
            }
            let request: Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
            let result = match request["method"].as_str().unwrap() {
                "peerStartAware" => {
                    let result = json!(count == 1);
                    count += 1;
                    result
                }
                "discoveryFacts" => json!({"name":"fixture tablet","deviceType":"TABLET"}),
                "peerTransportCapabilities" => json!(["BLE"]),
                "peerTransportBleExchange" => {
                    let wire = crate::services::discovery::ble_http::test_peer_wire(
                        &ble,
                        &request["params"],
                        &[7; 32],
                    );
                    let parts: Vec<_> = wire.splitn(3, '|').collect();
                    assert!(plain_rs::crypto::ed25519_verify(
                        &plain_rs::utils::base64::base64_encode(&local_public_key),
                        format!("{}{}", parts[1], parts[2]).as_bytes(),
                        parts[0]
                    ));
                    let document: Value = serde_json::from_str(parts[2]).unwrap();
                    assert_eq!(document["variables"]["type"], "UPDATE");
                    let payload: Value =
                        serde_json::from_str(document["variables"]["payload"].as_str().unwrap())
                            .unwrap();
                    let local = payload["memberPeers"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|p| p["id"] == "local")
                        .unwrap();
                    assert_eq!(local["name"], "fixture tablet");
                    assert_eq!(local["deviceType"], "TABLET");
                    if let Some(done) = broadcast_done.take() {
                        let _ = done.send(());
                    }
                    crate::services::discovery::ble_http::test_reply(
                        &ble,
                        &request["params"],
                        json!({"data":{"channelSystemMessage":true}}),
                        &[7; 32],
                    )
                }
                _ => panic!("Unexpected Host call: HTTPS must bypass httpExchange: {request}"),
            };
            host.send(Message::Text(
                json!({"id":request["id"],"result":result})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        }
    });
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .no_proxy()
        .build()
        .unwrap();
    let wire = |document: Value| {
        let content = document.to_string();
        let ts = plain_rs::peers::pairing::now_ms();
        let signature = plain_rs::crypto::ed25519_sign(&kp, format!("{ts}{content}").as_bytes());
        plain_rs::crypto::xchacha_encrypt_raw(
            &key,
            format!("{signature}|{ts}|{content}").as_bytes(),
        )
        .unwrap()
    };
    let public_url = format!("https://127.0.0.1:{https}/peer_graphql");
    let body = wire(
        json!({"query":"mutation Incoming($content:String!) { renamed:createChatItem(content:$content) { ...Fields } } fragment Fields on ChatItem { id fromId channelId content createdAt updatedAt status }", "operationName":"Incoming", "variables":{"content":json!({"type":"TEXT","value":{"text":"hello"}}).to_string()}}),
    );
    let response = client
        .post(&public_url)
        .header("c-id", "peer")
        .body(body.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let content =
        plain_rs::crypto::xchacha_decrypt_raw(&key, &response.bytes().await.unwrap()).unwrap();
    let content: Value = serde_json::from_slice(&content).unwrap();
    assert!(content.get("errors").is_none(), "{content}");
    let id = content["data"]["renamed"][0]["id"].as_str().unwrap();
    assert_eq!(messages::get(&db, id).unwrap().unwrap().from_id, "peer");
    assert!(content["data"]["renamed"][0]["channelId"].is_null());
    for expected in [false, true] {
        let response = client
            .post(&public_url)
            .header("c-id", "peer")
            .body(wire(json!({"query":"mutation { startAware }"})))
            .send()
            .await
            .unwrap();
        let content: Value = serde_json::from_slice(
            &plain_rs::crypto::xchacha_decrypt_raw(&key, &response.bytes().await.unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(content["data"]["startAware"], expected);
    }
    let response = client
        .post(&public_url)
        .header("c-id", "peer")
        .body(wire(
            json!({"query":"mutation { channelSystemMessage(type:INVITE,payload:\"invalid\") }"}),
        ))
        .send()
        .await
        .unwrap();
    let content: Value = serde_json::from_slice(
        &plain_rs::crypto::xchacha_decrypt_raw(&key, &response.bytes().await.unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(content["data"]["channelSystemMessage"], false);
    assert_eq!(
        client
            .post(&public_url)
            .header("c-id", "unknown")
            .body(wire(json!({"query":"mutation { startAware }"})))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let channel = plain_rs::chat::channels::state::create(&db, "local", "fixture").unwrap();
    plain_rs::chat::channels::state::apply(
        &db,
        "local",
        &channel.id,
        plain_rs::chat::channels::state::Action::Invite {
            peer: "peer".into(),
        },
    )
    .unwrap();
    let mut channel_events = server.runtime_state().events.subscribe();
    let response = client.post(&public_url).header("c-id","peer").body(wire(json!({"query":"mutation Accept($payload:String!) { channelSystemMessage(type:INVITE_ACCEPT,payload:$payload) }","variables":{"payload":json!({"channelId":channel.id,"publicKey":plain_rs::utils::base64::base64_encode(&public_key),"name":"joined","deviceType":"PHONE"}).to_string()}}))).send().await.unwrap();
    let content: Value = serde_json::from_slice(
        &plain_rs::crypto::xchacha_decrypt_raw(&key, &response.bytes().await.unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(content["data"]["channelSystemMessage"], true, "{content}");
    let event = tokio::time::timeout(std::time::Duration::from_secs(2), channel_events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        event.event_type,
        plain_rs::chat::events::WS_CHANNELS_UPDATED
    );
    let event: Value = serde_json::from_str(&event.payload).unwrap();
    assert_eq!(event["channels"][0]["id"], channel.id);
    assert_eq!(event["channels"][0]["members"][1]["status"], "JOINED");
    assert!(event["channels"][0].get("key").is_none());

    tokio::time::timeout(std::time::Duration::from_secs(2), broadcast_received)
        .await
        .unwrap()
        .unwrap();
    prefs.set_user("service", false).unwrap();
    assert_eq!(
        client
            .post(&public_url)
            .header("c-id", "peer")
            .body(wire(json!({"query":"mutation { startAware }"})))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    server.shutdown().await;
    responder.abort();
}
