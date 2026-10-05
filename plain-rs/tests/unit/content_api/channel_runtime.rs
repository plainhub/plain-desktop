use super::*;
#[cfg(feature = "http_transport")]
fn fixture() -> (tempfile::TempDir, super::super::ContentServer) {
    use std::sync::Arc;
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    let (key, public) = crate::ed25519_generate();
    prefs.set("client_id", "actor").unwrap();
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
fn add_peer(state: &ServerState, id: &str, paired: bool) {
    let mut peer = crate::db::DPeer::new(id, id, "", 2443, crate::chat::enums::DeviceType::Tablet);
    if paired {
        peer.key = crate::base64_encode(&[8; 32]);
    }
    peers::save(&state.db, &[peer], crate::db::chat_store::SaveMode::Insert).unwrap();
}
#[cfg(feature = "http_transport")]
#[tokio::test]
async fn http_mutations_own_identity_pending_members_and_owner_policy() {
    let (_dir, server) = fixture();
    let state = server.runtime_state();
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/chat/channel", server.port);
    assert_eq!(
        client
            .post(&url)
            .header("content-type", "application/json")
            .body(json!({"action":"create","name":"fixture"}).to_string())
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let result: Value = serde_json::from_str(
        &client
            .post(&url)
            .bearer_auth(crate::base64_encode(&[3; 32]))
            .header("content-type", "application/json")
            .body(json!({"action":"create","name":" fixture "}).to_string())
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
    )
    .unwrap();
    let id = result["result"]["channel"]["id"].as_str().unwrap();
    assert_eq!(result["result"]["channel"]["owner_id"], "actor");
    execute(
        &state,
        Request::Rename {
            id: id.into(),
            name: "renamed".into(),
        },
    )
    .await
    .unwrap();
    execute(
        &state,
        Request::Invite {
            id: id.into(),
            peer: "unknown".into(),
        },
    )
    .await
    .unwrap();
    let channel = get(&state, id).unwrap();
    assert_eq!(channel.version, 3);
    assert!(channel.members.contains("PENDING"));
    assert!(
        execute(
            &state,
            Request::Resend {
                id: id.into(),
                peer: "unknown".into()
            }
        )
        .await
        .is_err()
    );
    assert!(
        execute(&state, Request::Leave { id: id.into() })
            .await
            .is_err()
    );
    execute(&state, Request::Delete { id: id.into() })
        .await
        .unwrap();
    assert!(channels::get(&state.db, id).unwrap().is_none());
    server.shutdown().await;
}
#[cfg(feature = "http_transport")]
#[tokio::test]
async fn signed_invites_ack_keys_and_deleted_broadcast_use_rust_snapshot() {
    let (_dir, server) = fixture();
    let state = server.runtime_state();
    add_peer(&state, "paired", true);
    add_peer(&state, "group", false);
    let channel = state::create(&state.db, "actor", "fixture").unwrap();
    let (generation, mut rx) = state.host.connect();
    let host = state.host.clone();
    let prefs = state.prefs.clone();
    let channel_key = channel.key.clone();
    let channel_id = channel.id.clone();
    let responder = tokio::spawn(async move {
        let mut attempts = Vec::new();
        while attempts.len() < 5 {
            let req = rx.recv().await.unwrap();
            let result = match req["method"].as_str().unwrap() {
                "peerDeviceInfo" => json!({"name":"Actual tablet","deviceType":"TABLET"}),
                "peerTransportCapabilities" => json!(["AWARE"]),
                "peerTransportAttempt" => {
                    let p = &req["params"];
                    let body = p["body"].as_str().unwrap();
                    let pieces: Vec<_> = body.splitn(3, '|').collect();
                    let public = crate::base64_encode(
                        &super::super::peer_wire::signing_keypair(&prefs).unwrap()[32..],
                    );
                    assert!(crate::ed25519_verify(
                        &public,
                        format!("{}{}", pieces[1], pieces[2]).as_bytes(),
                        pieces[0]
                    ));
                    let wire: Value = serde_json::from_str(pieces[2]).unwrap();
                    let payload: Value =
                        serde_json::from_str(wire["variables"]["payload"].as_str().unwrap())
                            .unwrap();
                    if wire["variables"]["type"] == "INVITE" {
                        assert_eq!(payload["memberPeers"][0]["deviceType"], "TABLET");
                    }
                    if p["peer"]["id"] == "paired" {
                        assert_eq!(p["key"], crate::base64_encode(&[8; 32]));
                        assert_eq!(p["channelId"], "");
                    } else {
                        assert_eq!(p["key"], channel_key);
                        assert_eq!(p["channelId"], channel_id);
                    }
                    attempts.push(wire["variables"]["type"].as_str().unwrap().to_owned());
                    json!({"kind":"connected","response":{"data":{"channelSystemMessage": attempts.len()!=1}}})
                }
                method => panic!("Unexpected {method}"),
            };
            host.reply(generation, json!({"id":req["id"],"result":result}))
                .unwrap();
        }
        attempts
    });
    let first = execute(
        &state,
        Request::Invite {
            id: channel.id.clone(),
            peer: "paired".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        first["response"]["errors"][0]["message"],
        "Channel message rejected"
    );
    execute(
        &state,
        Request::Invite {
            id: channel.id.clone(),
            peer: "group".into(),
        },
    )
    .await
    .unwrap();
    execute(
        &state,
        Request::Send {
            id: channel.id.clone(),
            message_type: ChannelSystemMessageType::Update,
            target: "paired".into(),
        },
    )
    .await
    .unwrap();
    execute(
        &state,
        Request::Delete {
            id: channel.id.clone(),
        },
    )
    .await
    .unwrap();
    let attempts = responder.await.unwrap();
    assert_eq!(attempts.iter().filter(|s| s.as_str() == "KICK").count(), 2);
    assert!(channels::get(&state.db, &channel.id).unwrap().is_none());
    state.host.disconnect(generation);
    server.shutdown().await;
}
#[cfg(feature = "http_transport")]
#[tokio::test]
async fn stale_device_facts_and_decline_ack_cannot_act_on_replacement() {
    let (_dir, server) = fixture();
    let state = server.runtime_state();
    add_peer(&state, "owner", true);
    let mut channel = state::create(&state.db, "owner", "invitation").unwrap();
    channel.members = serde_json::to_string(&[
        crate::chat::channel::messages::ChannelMember::new("owner"),
        crate::chat::channel::messages::ChannelMember::pending("actor"),
    ])
    .unwrap();
    channels::save(
        &state.db,
        &[channel.clone()],
        crate::db::chat_store::SaveMode::Update,
    )
    .unwrap();
    let (generation, mut rx) = state.host.connect();
    let call = {
        let state = state.clone();
        let id = channel.id.clone();
        tokio::spawn(async move { execute(&state, Request::Accept { id }).await })
    };
    let facts = rx.recv().await.unwrap();
    assert_eq!(facts["method"], "peerDeviceInfo");
    let newer = state::apply(
        &state.db,
        "owner",
        &channel.id,
        Action::Rename {
            name: "new invitation".into(),
        },
    )
    .unwrap();
    state
        .host
        .reply(
            generation,
            json!({"id":facts["id"],"result":{"name":"Tablet","deviceType":"TABLET"}}),
        )
        .unwrap();
    let response = call.await.unwrap().unwrap();
    assert_eq!(
        response["response"]["errors"][0]["message"],
        "Channel changed before send"
    );
    let call = {
        let state = state.clone();
        let id = channel.id.clone();
        tokio::spawn(async move { execute(&state, Request::Decline { id }).await })
    };
    let capabilities = rx.recv().await.unwrap();
    state
        .host
        .reply(
            generation,
            json!({"id":capabilities["id"],"result":["AWARE"]}),
        )
        .unwrap();
    let attempt = rx.recv().await.unwrap();
    let replacement = state::apply(
        &state.db,
        "owner",
        &channel.id,
        Action::Rename {
            name: "replacement".into(),
        },
    )
    .unwrap();
    state.host.reply(generation,json!({"id":attempt["id"],"result":{"kind":"connected","response":{"data":{"channelSystemMessage":true}}}})).unwrap();
    assert!(
        call.await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("Channel changed")
    );
    assert_eq!(get(&state, &channel.id).unwrap(), replacement);
    assert_ne!(newer, replacement);
    state.host.disconnect(generation);
    server.shutdown().await;
}

#[cfg(feature = "http_transport")]
#[tokio::test]
async fn peer_key_changes_before_physical_send_are_rejected_and_accept_leave_decline_are_root_owned()
 {
    let (_dir, server) = fixture();
    let state = server.runtime_state();
    add_peer(&state, "owner", true);
    let mut channel = state::create(&state.db, "owner", "fixture").unwrap();
    channel.members = serde_json::to_string(&[
        crate::chat::channel::messages::ChannelMember::new("owner"),
        crate::chat::channel::messages::ChannelMember::pending("actor"),
    ])
    .unwrap();
    channels::save(
        &state.db,
        &[channel.clone()],
        crate::db::chat_store::SaveMode::Update,
    )
    .unwrap();
    let (generation, mut rx) = state.host.connect();
    let call = {
        let state = state.clone();
        let id = channel.id.clone();
        tokio::spawn(async move { execute(&state, Request::Accept { id }).await })
    };
    let facts = rx.recv().await.unwrap();
    state
        .host
        .reply(
            generation,
            json!({"id":facts["id"],"result":{"name":"Actual tablet","deviceType":"TABLET"}}),
        )
        .unwrap();
    let cap = rx.recv().await.unwrap();
    let mut peer = peers::get(&state.db, "owner").unwrap().unwrap();
    peer.key = crate::base64_encode(&[9; 32]);
    peers::save(&state.db, &[peer], crate::db::chat_store::SaveMode::Update).unwrap();
    state
        .host
        .reply(generation, json!({"id":cap["id"],"result":["AWARE"]}))
        .unwrap();
    assert_eq!(
        call.await.unwrap().unwrap()["response"]["errors"][0]["message"],
        "Peer changed before send"
    );
    let host = state.host.clone();
    let responder = tokio::spawn(async move {
        let mut kinds = Vec::new();
        while kinds.len() < 3 {
            let req = rx.recv().await.unwrap();
            let result = match req["method"].as_str().unwrap() {
                "peerDeviceInfo" => json!({"name":"Actual tablet","deviceType":"TABLET"}),
                "peerTransportCapabilities" => json!(["AWARE"]),
                "peerTransportAttempt" => {
                    assert_eq!(req["params"]["key"], crate::base64_encode(&[9; 32]));
                    let body = req["params"]["body"].as_str().unwrap();
                    let wire: Value =
                        serde_json::from_str(body.splitn(3, '|').nth(2).unwrap()).unwrap();
                    let kind = wire["variables"]["type"].as_str().unwrap().to_string();
                    if kind == "INVITE_ACCEPT" {
                        let payload: Value =
                            serde_json::from_str(wire["variables"]["payload"].as_str().unwrap())
                                .unwrap();
                        assert_eq!(payload["deviceType"], "TABLET");
                        assert_eq!(payload["name"], "Actual tablet");
                    }
                    kinds.push(kind);
                    json!({"kind":"connected","response":{"data":{"channelSystemMessage":true}}})
                }
                method => panic!("Unexpected {method}"),
            };
            host.reply(generation, json!({"id":req["id"],"result":result}))
                .unwrap();
        }
        kinds
    });
    let accepted = execute(
        &state,
        Request::Accept {
            id: channel.id.clone(),
        },
    )
    .await
    .unwrap();
    assert_eq!(accepted["response"]["data"]["channelSystemMessage"], true);
    assert!(
        get(&state, &channel.id)
            .unwrap()
            .members
            .contains("PENDING")
    );
    execute(
        &state,
        Request::Leave {
            id: channel.id.clone(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        get(&state, &channel.id).unwrap().status,
        crate::chat::enums::ChannelStatus::Left
    );
    // An owner re-invitation replaces the left state, then decline removes exactly that row.
    channels::save(
        &state.db,
        &[channel.clone()],
        crate::db::chat_store::SaveMode::Update,
    )
    .unwrap();
    execute(
        &state,
        Request::Decline {
            id: channel.id.clone(),
        },
    )
    .await
    .unwrap();
    assert!(channels::get(&state.db, &channel.id).unwrap().is_none());
    assert_eq!(
        responder.await.unwrap(),
        vec!["INVITE_ACCEPT", "LEAVE", "INVITE_DECLINE"]
    );
    state.host.disconnect(generation);
    server.shutdown().await;
}
