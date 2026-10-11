use super::*;
use std::sync::Arc;
async fn value(response: Response) -> Value {
    assert_eq!(response.status(), StatusCode::OK);
    serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap()
}
#[tokio::test]
async fn authenticated_local_commands_commit_events_and_clear_without_transport() {
    let dir = tempfile::tempdir().unwrap();
    let token = plain_rs::utils::base64::base64_encode(&[1; 32]);
    let prefs = Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    let server =
        crate::assembly::server::ContentServer::start(&dir.path().join("plain.db"), &token, prefs)
            .unwrap();
    let state = server.runtime_state();
    let mut events = state.events.subscribe();
    let mut headers = HeaderMap::new();
    headers.insert("authorization", format!("Bearer {token}").parse().unwrap());
    let rejected = call(
        State(state.clone()),
        HeaderMap::new(),
        Json(Request::CreateFiles {
            target: "peer:local".into(),
            items: vec![],
            images: false,
        }),
    )
    .await;
    assert_eq!(rejected.status(), StatusCode::UNAUTHORIZED);
    assert!(
        plain_rs::db::chat_store::messages::all(&state.db)
            .unwrap()
            .is_empty()
    );
    let created = value(
        call(
            State(state.clone()),
            headers.clone(),
            Json(Request::CreateFiles {
                target: "peer:local".into(),
                items: vec![],
                images: false,
            }),
        )
        .await,
    )
    .await;
    let id = created["result"]["id"].as_str().unwrap().to_owned();
    assert_eq!(
        events.recv().await.unwrap().event_type,
        plain_rs::chat::events::WS_MESSAGE_CREATED
    );
    let saved = value(
        call(
            State(state.clone()),
            headers.clone(),
            Json(Request::ReplaceFilesMany {
                ids: vec![id.clone(), "missing".into()],
                items: vec![],
            }),
        )
        .await,
    )
    .await;
    assert_eq!(saved["result"][0]["status"], "SENT");
    assert!(saved["result"][1].is_null());
    assert_eq!(
        events.recv().await.unwrap().event_type,
        plain_rs::chat::events::WS_MESSAGE_UPDATED
    );
    let cleared = value(
        call(
            State(state.clone()),
            headers,
            Json(Request::Clear {
                target: "peer:local".into(),
            }),
        )
        .await,
    )
    .await;
    assert_eq!(cleared["result"], 1);
    assert!(
        plain_rs::db::chat_store::messages::get(&state.db, &id)
            .unwrap()
            .is_none()
    );
    let event = events.recv().await.unwrap();
    assert_eq!(event.event_type, plain_rs::chat::events::WS_MESSAGE_DELETED);
    assert_eq!(
        serde_json::from_str::<Value>(&event.payload).unwrap(),
        json!("peer:local")
    );
}

#[cfg(feature = "http_transport")]
mod attachments {
    use crate::assembly::server::ContentServer;
    use serde_json::{Value, json};
    use std::sync::Arc;
    fn root(id: &str) -> (tempfile::TempDir, ContentServer, Vec<u8>, String) {
        let dir = tempfile::tempdir().unwrap();
        let prefs =
            Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
        prefs.set("client_id", id).unwrap();
        prefs.set_user("service", true).unwrap();
        let (kp, pk) = plain_rs::crypto::ed25519_generate();
        prefs
            .set(
                "signature_key_pair",
                plain_rs::utils::base64::base64_encode(&kp),
            )
            .unwrap();
        let token = plain_rs::utils::base64::base64_encode(&[3; 32]);
        let server = ContentServer::start(&dir.path().join("plain.db"), &token, prefs).unwrap();
        (dir, server, kp.to_vec(), token)
    }
    async fn call(server: &ContentServer, token: &str, body: Value) -> Value {
        let response = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}/chat/service", server.port))
            .bearer_auth(token)
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .unwrap();
        let status = response.status();
        let value: Value = serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
        assert_eq!(status, 200, "{value}");
        value["result"].clone()
    }
    #[tokio::test]
    async fn prepared_attachments_send_without_host_and_cleanup_preserves_message_owners() {
        let (dir, server, _, token) = root("local");
        let state = server.runtime_state();
        let source = dir.path().join("fixture.txt");
        std::fs::write(&source, b"abc").unwrap();
        let store = plain_rs::files::app_files::FileStore::new(state.db.clone(), dir.path().into());
        let file = store
            .import(
                source.clone(),
                "fixture.txt".into(),
                "text/plain".into(),
                false,
            )
            .await
            .unwrap();
        let metadata = plain_rs::chat::messages::share_send::Attachment {
            file_id: file.id.clone(),
            name: "fixture.opaque".into(),
            width: 12,
            height: 34,
            duration_ms: 56,
        };
        let items = plain_rs::chat::messages::share_send::attachments(&state.db, &[metadata], true)
            .unwrap();
        assert_eq!(items[0]["fileName"], "fixture.txt");
        assert_eq!(items[0]["width"], 12);
        assert_eq!(items[0]["height"], 34);
        assert_eq!(items[0]["durationMs"], 56);
        let attachment =
            json!({"fileId":file.id,"name":"fixture.txt","width":0,"height":0,"durationMs":0});
        let result = call(&server, &token, json!({"action":"share","targets":["local","local"],
        "attachments":[attachment],"text":null,"caption":"caption","images":null,"normalize":false})).await;
        assert_eq!(result["ok"], true);
        assert_eq!(result["chats"].as_array().unwrap().len(), 4);
        let content: Value =
            serde_json::from_str(result["chats"][0]["content"].as_str().unwrap()).unwrap();
        assert_eq!(content["value"]["items"][0]["size"], 3);
        assert!(
            content["value"]["items"][0]["uri"]
                .as_str()
                .unwrap()
                .starts_with("fid:")
        );
        store.release(file.id.clone()).await.unwrap();
        assert_eq!(
            state.db.app_file_get(&file.id).unwrap().unwrap().ref_count,
            2
        );
        let reserved = store
            .import(source, "fixture.txt".into(), "text/plain".into(), false)
            .await
            .unwrap();
        assert_eq!(reserved.ref_count, 3);
        store.release(file.id.clone()).await.unwrap();
        assert_eq!(
            state.db.app_file_get(&file.id).unwrap().unwrap().ref_count,
            2
        );
        for row in &result["chats"].as_array().unwrap()[..2] {
            call(
                &server,
                &token,
                json!({"action":"delete","ids":[row["id"]]}),
            )
            .await;
        }
        assert!(state.db.app_file_get(&file.id).unwrap().is_none());
        let unused = store
            .import(
                dir.path().join("fixture.txt"),
                "fixture.txt".into(),
                "text/plain".into(),
                false,
            )
            .await
            .unwrap();
        store.release(unused.id.clone()).await.unwrap();
        assert!(state.db.app_file_get(&unused.id).unwrap().is_none());
        server.shutdown().await;
    }
    #[tokio::test]
    async fn share_rejects_platform_sources_and_unknown_attachment_references() {
        let (_dir, server, _, token) = root("local");
        for body in [
            json!({"action":"share","targets":["local"],"uris":["content://fixture"],"text":null,"caption":null,"images":false,"normalize":false}),
            json!({"action":"share","targets":["local"],"attachments":[{"fileId":"content://fixture","name":"fixture.txt","width":0,"height":0,"durationMs":0}],"text":null,"caption":null,"images":false,"normalize":false}),
        ] {
            let response = reqwest::Client::new()
                .post(format!("http://127.0.0.1:{}/chat/service", server.port))
                .bearer_auth(&token)
                .header("content-type", "application/json")
                .body(body.to_string())
                .send()
                .await
                .unwrap();
            assert!(!response.status().is_success());
            assert!(server.runtime_state().db.get_all_chats().is_empty());
        }
        server.shutdown().await;
    }
}
