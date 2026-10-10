use super::*;
use crate::content_api::host::Host;
use crate::content_api::server::ServerState;

struct PublicSchema {
    state: ServerState,
    _server: super::super::ContentServer,
}
use crate::prefs::Prefs;
use serde_json::{Value, json};
use std::sync::Arc;

fn stub(host: Arc<Host>, handler: impl Fn(&str, Value) -> Value + Send + 'static) {
    let (generation, mut requests) = host.connect();
    let host = host.clone();
    tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let Some(id) = request["id"].as_u64() else {
                continue;
            };
            let result = handler(
                &request["method"].as_str().unwrap_or_default(),
                request["params"].clone(),
            );
            let _ = host.reply(generation, json!({ "id": id, "result": result }));
        }
    });
}

fn fixture_with(
    handler: impl Fn(&str, Value) -> Value + Send + 'static,
) -> (tempfile::TempDir, PublicSchema) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&dir.path().join("prefs.json")).unwrap());
    prefs.set("client_id", "actor").unwrap();
    let server = super::super::ContentServer::start(
        &dir.path().join("data.db"),
        &crate::base64_encode(&[3; 32]),
        prefs,
    )
    .unwrap();
    let state = server.runtime_state();
    let mut peer = crate::db::DPeer::new(
        "peer-1",
        "Pixel",
        "192.168.1.20",
        1234,
        crate::chat::enums::DeviceType::Phone,
    );
    peer.status = crate::chat::enums::PeerStatus::Paired;
    crate::db::chat_store::peers::save(&state.db, &[peer], crate::db::chat_store::SaveMode::Insert)
        .unwrap();
    state
        .peer_status
        .connections
        .hint(&state.db, "peer-1")
        .unwrap();
    stub(state.host.clone(), handler);
    (
        dir,
        PublicSchema {
            state,
            _server: server,
        },
    )
}

async fn query(schema: &PublicSchema, document: &str) -> Value {
    let bytes = super::super::main_graphql::run(
        &schema.state.public,
        &json!({"query":document}).to_string(),
        Some(schema.state.clone()),
    )
    .await
    .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn fixture() -> (tempfile::TempDir, PublicSchema) {
    fixture_with(|method, _| match method {
        "systemSimFacts" => json!([
            { "id": "sim-0", "label": "SIM 1", "number": "+15550100", "subscriptionId": 1 },
        ]),
        "discoveryFacts" => {
            json!({"name":"local","deviceType":"PHONE","version":"1","platform":"android","ips":[],"awareSupported":false,"awareRunning":false})
        }
        _other => panic!("unexpected host call {_other}"),
    })
}

#[tokio::test]
async fn a_peer_reports_its_liveness_and_kind() {
    let (_dir, schema) = fixture();
    let listed = query(
        &schema,
        r#"query { peers { id name ip status port deviceType createdAt updatedAt online } }"#,
    )
    .await;
    let rows = listed["data"]["peers"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{listed}");
    assert_eq!(rows[0]["status"], "PAIRED");
    assert_eq!(rows[0]["deviceType"], "PHONE");
    assert_eq!(rows[0]["online"], true);
    assert_eq!(rows[0]["port"], 1234);
}

/// A device with no telephony has no SIMs; an empty list is the answer the
/// contract expects, not an error the settings screen would have to catch.
#[tokio::test]
async fn sims_carry_the_slot_the_sender_chooses() {
    let (_dir, schema) = fixture();
    let listed = query(
        &schema,
        r#"query { sims { id label number subscriptionId } }"#,
    )
    .await;
    let rows = listed["data"]["sims"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{listed}");
    assert_eq!(rows[0]["subscriptionId"], 1);
    assert_eq!(rows[0]["number"], "+15550100");
}

#[tokio::test]
async fn an_empty_sim_list_is_not_an_error() {
    let (_dir, schema) = fixture_with(|method, _| match method {
        "systemSimFacts" => json!([]),
        _other => panic!("unexpected host call {_other}"),
    });
    let listed = query(&schema, r#"query { sims { id } }"#).await;
    assert!(
        listed["errors"].as_array().is_none_or(Vec::is_empty),
        "{listed}"
    );
    assert_eq!(listed["data"]["sims"].as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn pairing_sends_the_device_the_contract_described() {
    let (_dir, schema) = fixture();
    let paired = query(
        &schema,
        r#"mutation { pairDevice(input: {
            id: "peer-9", name: "Tablet", ips: [], port: 4321,
            deviceType: TABLET, version: "1.2.3", platform: "android",
            lastSeen: "2026-01-02T03:04:05+00:00", discoveryMethods: [LAN, BLE]
        }) }"#,
    )
    .await;
    assert_eq!(paired["data"]["pairDevice"], true, "{paired}");
}

#[tokio::test]
async fn responding_to_a_pairing_request_passes_the_whole_handshake() {
    let (_dir, schema) = fixture();
    let responded = query(
        &schema,
        r#"mutation { respondToPairing(input: {
            fromId: "peer-9", fromName: "Tablet", port: 4321, deviceType: TABLET,
            ecdhPublicKey: "ecdh", signaturePublicKey: "sig", timestamp: 1767322,
            ips: ["10.0.0.5"], signature: "signed", fromIp: "10.0.0.5",
            awareSupported: true
        }, accepted: true) }"#,
    )
    .await;
    assert_eq!(responded["data"]["respondToPairing"], true, "{responded}");
}

#[tokio::test]
async fn cancelling_and_unpairing_both_acknowledge() {
    let (_dir, schema) = fixture();
    for (document, field) in [
        (
            r#"mutation { cancelPairing(deviceId: "peer-9") }"#,
            "cancelPairing",
        ),
        (r#"mutation { unpairPeer(id: "peer-1") }"#, "unpairPeer"),
        (r#"mutation { deletePeer(id: "peer-1") }"#, "deletePeer"),
    ] {
        let result = query(&schema, document).await;
        assert!(
            result["errors"].as_array().is_none_or(Vec::is_empty),
            "{result}"
        );
        assert_eq!(result["data"][field], true, "{result}");
    }
}

#[tokio::test]
async fn unsupported_transport_does_not_call_native_pairing() {
    let (_dir, schema) = fixture();
    let mut events = schema.state.events.subscribe();
    let paired = query(
        &schema,
        r#"mutation { pairDevice(input: {
        id: "peer-9", name: "Tablet", ips: [], port: 4321,
        deviceType: TABLET, version: "1", platform: "android",
        lastSeen: "2026-01-02T03:04:05+00:00", discoveryMethods: [LAN, QR]
    }) }"#,
    )
    .await;
    assert!(
        paired["errors"].as_array().is_none_or(Vec::is_empty),
        "{paired}"
    );
    assert_eq!(
        events.recv().await.unwrap().event_type,
        crate::chat::events::WS_PAIRING_FAILED
    );
}
