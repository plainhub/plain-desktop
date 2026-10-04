use super::*;
use std::sync::Arc;
fn fixture(id: &str) -> (tempfile::TempDir, super::super::ContentServer) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap());
    let (key, public) = crate::ed25519_generate();
    prefs.set("client_id", id).unwrap();
    prefs.set("signature_key_pair",json!({"privateKey":crate::base64_encode(&key[..32]),"publicKey":crate::base64_encode(&public)}).to_string()).unwrap();
    let server = super::super::ContentServer::start(
        &dir.path().join("data.db"),
        &crate::base64_encode(&[3; 32]),
        prefs,
    )
    .unwrap();
    (dir, server)
}
fn device() -> Device {
    Device {
        name: "Fixture".into(),
        port: 2443,
        device_type: "PHONE".into(),
        ips: vec![],
        aware_supported: false,
    }
}
fn target(id: &str) -> Target {
    Target {
        device_id: id.into(),
        device_name: id.into(),
        device_ip: "spoofed".into(),
        device_port: 2443,
    }
}
async fn idle(state: &ServerState) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if state.ble_pairing.0.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn ble_initiator_signs_filters_notifications_commits_shared_key_and_closes_once() {
    let (_d, local) = fixture("local");
    let (_r, remote) = fixture("remote");
    let state = local.runtime_state();
    let other = remote.runtime_state();
    let mut events = state.events.subscribe();
    let (generation, mut effects) = state.host.connect();
    let host = state.host.clone();
    let remote_state = other.clone();
    let local_state = state.clone();
    let driver = tokio::spawn(async move {
        let mut body = None;
        let mut step = 0;
        let mut response = None;
        while let Some(effect) = effects.recv().await {
            let result = match effect["method"].as_str().unwrap() {
                "blePairConnect" => {
                    assert_eq!(effect["params"]["id"], "remote");
                    json!(true)
                }
                "blePairSend" => {
                    let request =
                        match Message::parse(effect["params"]["body"].as_str().unwrap()).unwrap() {
                            Message::PairRequest(r) => r,
                            _ => panic!(),
                        };
                    assert!(crate::chat::pairing::security::verify_request(&request));
                    body = Some(request.clone());
                    pairing::receive_request(&remote_state.pairing, &request).unwrap();
                    let result = pairing::respond(
                        &remote_state.db,
                        &remote_state.prefs,
                        &remote_state.pairing,
                        request,
                        true,
                        device(),
                    )
                    .unwrap();
                    response = Some(serde_json::from_value(result["response"].clone()).unwrap());
                    json!(true)
                }
                "blePairWait" => {
                    assert_eq!(local_state.pairing.states()[0].1, "PAIRING");
                    assert!(effect["params"]["timeoutMs"].as_u64().unwrap() <= 1000);
                    step += 1;
                    let notification = match step {
                        1 => "malformed".into(),
                        2 => {
                            let mut wrong: crate::chat::pairing::protocol::PairingResponse =
                                response.clone().unwrap();
                            wrong.from_id = "unrelated".into();
                            Message::PairResponse(wrong).wire().unwrap()
                        }
                        3 => {
                            let mut invalid: crate::chat::pairing::protocol::PairingResponse =
                                response.clone().unwrap();
                            invalid.signature = "invalid".into();
                            Message::PairResponse(invalid).wire().unwrap()
                        }
                        _ => Message::PairResponse(response.clone().unwrap())
                            .wire()
                            .unwrap(),
                    };
                    json!({"connected":true,"notification":notification})
                }
                "blePairClose" => {
                    assert!(body.is_some());
                    assert_eq!(step, 4);
                    host.reply(generation, json!({"id":effect["id"],"result":true}))
                        .unwrap();
                    break;
                }
                method => panic!("Unexpected method {method}"),
            };
            host.reply(generation, json!({"id":effect["id"],"result":result}))
                .unwrap();
        }
    });
    let first = start(&state, target("remote"), device()).unwrap();
    assert_eq!(state.pairing.states()[0].1, "STARTING");
    let duplicate = start(&state, target("remote"), device()).unwrap();
    assert_eq!(
        first["ticket"]["generation"],
        duplicate["ticket"]["generation"]
    );
    assert_eq!(first["ticket"]["deviceIp"], "");
    tokio::time::timeout(Duration::from_secs(3), driver)
        .await
        .unwrap()
        .unwrap();
    idle(&state).await;
    let local_peer = crate::db::chat_store::peers::get(&state.db, "remote")
        .unwrap()
        .unwrap();
    let remote_peer = crate::db::chat_store::peers::get(&other.db, "local")
        .unwrap()
        .unwrap();
    assert_eq!(local_peer.key, remote_peer.key);
    assert!(local_peer.is_paired());
    assert!(state.pairing.tickets().is_empty());
    assert_eq!(events.recv().await.unwrap().event_type, WS_PAIRING_STARTED);
    assert_eq!(
        events.recv().await.unwrap().event_type,
        crate::chat::events::WS_PAIRING_SUCCESS
    );
    local.shutdown().await;
    remote.shutdown().await;
}
#[tokio::test]
async fn cancellation_interrupts_pending_connect_and_preserves_replacement_generation() {
    let (_dir, server) = fixture("local");
    let state = server.runtime_state();
    let (generation, mut effects) = state.host.connect();
    let old = start(&state, target("remote"), device()).unwrap();
    let connect = effects.recv().await.unwrap();
    assert_eq!(connect["method"], "blePairConnect");
    let replacement =
        pairing::start(&state.prefs, &state.pairing, target("remote"), device()).unwrap();
    let close = tokio::time::timeout(Duration::from_secs(2), effects.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(close["method"], "blePairClose");
    assert_eq!(close["params"]["generation"], old["ticket"]["generation"]);
    state
        .host
        .reply(generation, json!({"id":connect["id"],"result":true}))
        .unwrap();
    state
        .host
        .reply(generation, json!({"id":close["id"],"result":true}))
        .unwrap();
    idle(&state).await;
    assert_eq!(
        state.pairing.tickets()[0].generation,
        replacement["ticket"]["generation"].as_str().unwrap()
    );
    assert!(
        crate::db::chat_store::peers::get(&state.db, "remote")
            .unwrap()
            .is_none()
    );
    let (_remote_dir, remote) = fixture("remote");
    let other = remote.runtime_state();
    let request = serde_json::from_value(replacement["request"].clone()).unwrap();
    pairing::receive_request(&other.pairing, &request).unwrap();
    let rejected = pairing::respond(
        &other.db,
        &other.prefs,
        &other.pairing,
        request,
        false,
        device(),
    )
    .unwrap();
    let response = serde_json::from_value(rejected["response"].clone()).unwrap();
    assert!(
        pairing::complete_checked(
            &state.db,
            &state.prefs,
            &state.pairing,
            response,
            "",
            old["ticket"]["generation"].as_str()
        )
        .unwrap()
        .is_null()
    );
    assert_eq!(
        state.pairing.tickets()[0].generation,
        replacement["ticket"]["generation"].as_str().unwrap()
    );
    remote.shutdown().await;
    server.shutdown().await;
}
#[tokio::test]
async fn capacity_and_actual_connect_refusal_release_only_their_ticket() {
    let (_dir, server) = fixture("local");
    let state = server.runtime_state();
    let (generation, mut effects) = state.host.connect();
    start(&state, target("first"), device()).unwrap();
    start(&state, target("second"), device()).unwrap();
    assert!(start(&state, target("third"), device()).unwrap().is_null());
    assert_eq!(state.pairing.tickets().len(), 2);
    for _ in 0..4 {
        let effect = tokio::time::timeout(Duration::from_secs(2), effects.recv())
            .await
            .unwrap()
            .unwrap();
        let result = match effect["method"].as_str().unwrap() {
            "blePairConnect" => false,
            "blePairClose" => true,
            _ => panic!(),
        };
        state
            .host
            .reply(generation, json!({"id":effect["id"],"result":result}))
            .unwrap();
    }
    idle(&state).await;
    assert!(state.pairing.tickets().is_empty());
    assert!(
        crate::db::chat_store::peers::all(&state.db)
            .unwrap()
            .is_empty()
    );
    server.shutdown().await;
}
