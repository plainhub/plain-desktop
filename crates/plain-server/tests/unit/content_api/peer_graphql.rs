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
    use crate::{
        chat::enums::{DeviceType, PeerStatus},
        content_api::ContentServer,
        db::{
            DPeer, Db,
            chat_store::{SaveMode, messages, peers},
        },
    };
    use futures_util::{SinkExt, StreamExt};
    use std::sync::Arc;
    use tokio_tungstenite::{
        connect_async,
        tungstenite::{Message, client::IntoClientRequest},
    };
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    prefs.set_user("service", true).unwrap();
    prefs.set("client_id", "local").unwrap();
    let (local_keypair, local_public_key) = crate::ed25519_generate();
    prefs.set("signature_key_pair", json!({"privateKey":crate::base64_encode(&local_keypair[..32]),"publicKey":crate::base64_encode(&local_public_key)}).to_string()).unwrap();
    let token = crate::base64_encode(&[5; 32]);
    let db = Db::open(&dir.path().join("plain.db")).unwrap();
    let key = [7; 32];
    let (kp, public_key) = crate::ed25519_generate();
    let mut peer = DPeer::new("peer", "phone", "", 443, DeviceType::Phone);
    peer.status = PeerStatus::Paired;
    peer.key = crate::base64_encode(&key);
    peer.public_key = crate::base64_encode(&public_key);
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
    let mut request = format!("ws://127.0.0.1:{}/host", server.port)
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    let (mut host, _) = connect_async(request).await.unwrap();
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
                "peerDeviceInfo" => json!({"name":"fixture tablet","deviceType":"TABLET"}),
                "peerTransportCapabilities" => json!(["BLE"]),
                "peerTransportBleExchange" => {
                    let wire = crate::content_api::ble_http::test_peer_wire(
                        &ble,
                        &request["params"],
                        &[7; 32],
                    );
                    let parts: Vec<_> = wire.splitn(3, '|').collect();
                    assert!(crate::ed25519_verify(
                        &crate::base64_encode(&local_public_key),
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
                    crate::content_api::ble_http::test_reply(
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
        let ts = crate::chat::pairing::now_ms();
        let signature = crate::ed25519_sign(&kp, format!("{ts}{content}").as_bytes());
        crate::xchacha_encrypt_raw(&key, format!("{signature}|{ts}|{content}").as_bytes()).unwrap()
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
    let content = crate::xchacha_decrypt_raw(&key, &response.bytes().await.unwrap()).unwrap();
    let content: Value = serde_json::from_slice(&content).unwrap();
    assert!(content.get("errors").is_none(), "{content}");
    let id = content["data"]["renamed"][0]["id"].as_str().unwrap();
    assert_eq!(messages::get(&db, id).unwrap().unwrap().from_id, "peer");
    assert!(content["data"]["renamed"][0]["channelId"].is_null());
    let rpc_url = format!("http://127.0.0.1:{}/chat/peer-graphql", server.port);
    let rpc = |body: Vec<u8>| json!({"clientId":"peer","channelId":"","body":crate::base64_encode(&body)});
    assert_eq!(
        client
            .post(&rpc_url)
            .header("content-type", "application/json")
            .body(rpc(body.clone()).to_string())
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let response = client
        .post(&rpc_url)
        .bearer_auth(&token)
        .header("content-type", "application/json")
        .body(rpc(body).to_string())
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let response: Value = serde_json::from_str(&response).unwrap();
    assert_eq!(response["result"]["status"], 200);
    let content: Value = serde_json::from_slice(
        &crate::xchacha_decrypt_raw(
            &key,
            &crate::base64_decode(response["result"]["body"].as_str().unwrap()),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        content["data"]["renamed"],
        json!([]),
        "replay across transports is empty"
    );
    for expected in [false, true] {
        let response = client
            .post(&public_url)
            .header("c-id", "peer")
            .body(wire(json!({"query":"mutation { startAware }"})))
            .send()
            .await
            .unwrap();
        let content: Value = serde_json::from_slice(
            &crate::xchacha_decrypt_raw(&key, &response.bytes().await.unwrap()).unwrap(),
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
        &crate::xchacha_decrypt_raw(&key, &response.bytes().await.unwrap()).unwrap(),
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
    let channel = crate::chat::channel::state::create(&db, "local", "fixture").unwrap();
    crate::chat::channel::state::apply(
        &db,
        "local",
        &channel.id,
        crate::chat::channel::state::Action::Invite {
            peer: "peer".into(),
        },
    )
    .unwrap();
    let mut channel_events = server.runtime_state().events.subscribe();
    let response = client.post(&public_url).header("c-id","peer").body(wire(json!({"query":"mutation Accept($payload:String!) { channelSystemMessage(type:INVITE_ACCEPT,payload:$payload) }","variables":{"payload":json!({"channelId":channel.id,"publicKey":crate::base64_encode(&public_key),"name":"joined","deviceType":"PHONE"}).to_string()}}))).send().await.unwrap();
    let content: Value = serde_json::from_slice(
        &crate::xchacha_decrypt_raw(&key, &response.bytes().await.unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(content["data"]["channelSystemMessage"], true, "{content}");
    let event = tokio::time::timeout(std::time::Duration::from_secs(2), channel_events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event.event_type, crate::chat::events::WS_CHANNELS_UPDATED);
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
    let result = client
        .post(&rpc_url)
        .bearer_auth(&token)
        .header("content-type", "application/json")
        .body(rpc(wire(json!({"query":"mutation { startAware }"}))).to_string())
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let result: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(result["result"]["status"], 200);
    server.shutdown().await;
    responder.abort();
}
