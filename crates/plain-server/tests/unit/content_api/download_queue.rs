#![cfg(feature = "http_transport")]
use super::*;
use serde_json::Value;
use std::sync::Arc;
fn fixture() -> (tempfile::TempDir, super::super::ContentServer, ServerState) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    prefs.set("client_id", "actor").unwrap();
    let server = super::super::ContentServer::start(
        &dir.path().join("plain.db"),
        &crate::base64_encode(&[1; 32]),
        prefs,
    )
    .unwrap();
    let state = server.runtime_state();
    let peer = crate::db::DPeer::new(
        "peer",
        "fixture",
        "",
        443,
        crate::chat::enums::DeviceType::Phone,
    );
    crate::db::chat_store::peers::save(&state.db, &[peer], crate::db::chat_store::SaveMode::Insert)
        .unwrap();
    (dir, server, state)
}
fn seed_chat(state: &ServerState, id: &str, size: usize) {
    let row=crate::db::DChat::new("peer","me","",&json!({"type":"FILES","value":{"items":[{"id":id,"uri":format!("fsid:{id}"),"fileName":"fixture.bin","size":size}]}}).to_string());
    let mut row = row;
    row.id = id.into();
    state.db.insert_chat(&row);
}
async fn terminal(state: &ServerState, id: &str) -> Value {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let row = state.downloads.snapshot()["tasks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["id"] == id)
                .unwrap()
                .clone();
            if matches!(row["status"].as_str(), Some("COMPLETED" | "FAILED")) {
                return row;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap()
}
#[tokio::test]
async fn root_ble_chunks_commit_exact_bytes_and_reject_short_and_overlong_reads() {
    let (dir, server, state) = fixture();
    let payload = Arc::new((0..16384).map(|n| (n % 251) as u8).collect::<Vec<_>>());
    let (generation, mut requests) = state.host.connect();
    let host = state.host.clone();
    let data = payload.clone();
    let ble = state.ble_transport.clone();
    let responder = tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let result = match request["method"].as_str().unwrap() {
                "peerTransportCapabilities" => json!(["BLE"]),
                "peerTransportBleExchange" => {
                    let crate::content_api::ble_wire::Request::FileChunk {
                        file_id,
                        offset,
                        length,
                        ..
                    } = ble.request_for_test(&request["params"])
                    else {
                        panic!("Expected file request")
                    };
                    assert_eq!(length, 8192);
                    let offset = offset as usize;
                    let id = file_id.as_str();
                    let bytes = if id == "short" {
                        b"short".as_slice()
                    } else if id == "overlong" {
                        b"overlong".as_slice()
                    } else {
                        &data[offset..data.len().min(offset + 8192)]
                    };
                    ble.reply_for_test(
                        &request["params"],
                        crate::content_api::ble_wire::response(200, bytes).unwrap(),
                    )
                }
                "peerTransportSocketCloseAll" => json!(true),
                _ => panic!("{request}"),
            };
            host.reply(generation, json!({"id":request["id"],"result":result}))
                .unwrap();
        }
    });
    seed_chat(&state, "exact", payload.len());
    state.downloads.enqueue("exact", "exact", "peer").unwrap();
    let row = terminal(&state, "exact").await;
    assert_eq!(row["status"], "COMPLETED");
    assert_eq!(row["downloaded"], payload.len());
    let chat = crate::db::chat_store::messages::get(&state.db, "exact")
        .unwrap()
        .unwrap();
    let content: Value = serde_json::from_str(&chat.content).unwrap();
    let uri = content["value"]["items"][0]["uri"].as_str().unwrap();
    assert!(uri.starts_with("fid:"));
    let hash = uri[4..].split('.').next().unwrap();
    let record = state.db.app_file_get(hash).unwrap().unwrap();
    assert_eq!(
        std::fs::read(dir.path().join(record.real_path)).unwrap(),
        *payload
    );
    for (id, size) in [("short", 20), ("overlong", 3)] {
        seed_chat(&state, id, size);
        state.downloads.enqueue(id, id, "peer").unwrap();
        let row = terminal(&state, id).await;
        assert_eq!(row["status"], "FAILED");
        assert!(
            !dir.path()
                .join("attachment-transfers")
                .join(row["generation"].as_str().unwrap())
                .exists()
        );
        let row = crate::db::chat_store::messages::get(&state.db, id)
            .unwrap()
            .unwrap();
        assert!(row.content.contains("fsid:"));
    }
    server.shutdown().await;
    state.host.disconnect(generation);
    responder.abort();
}
#[tokio::test]
async fn paused_generation_and_shutdown_reject_late_bytes_and_release_transfer_files() {
    let (dir, server, state) = fixture();
    seed_chat(&state, "slow", 8192);
    let (generation, mut requests) = state.host.connect();
    state.downloads.enqueue("slow", "slow", "peer").unwrap();
    let caps = requests.recv().await.unwrap();
    state
        .host
        .reply(generation, json!({"id":caps["id"],"result":["BLE"]}))
        .unwrap();
    let old = requests.recv().await.unwrap();
    let row = state.downloads.snapshot()["tasks"][0].clone();
    state.downloads.control("slow", "pause").unwrap();
    state.host.reply(generation,json!({"id":old["id"],"result":json!({"s":200,"b":crate::base64_encode(&vec![1;8192])}).to_string()})).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    assert_eq!(state.downloads.snapshot()["tasks"][0]["status"], "PAUSED");
    assert!(
        !state
            .downloads
            .finish("slow", row["generation"].as_str().unwrap(), None)
            .unwrap()
    );
    state.downloads.control("slow", "resume").unwrap();
    let caps = requests.recv().await.unwrap();
    state
        .host
        .reply(generation, json!({"id":caps["id"],"result":["BLE"]}))
        .unwrap();
    let _exchange = requests.recv().await.unwrap();
    state.host.disconnect(generation);
    server.shutdown().await;
    assert!(state.transport.active().is_empty());
    assert_eq!(
        std::fs::read_dir(dir.path().join("attachment-transfers"))
            .unwrap()
            .count(),
        0
    );
}
