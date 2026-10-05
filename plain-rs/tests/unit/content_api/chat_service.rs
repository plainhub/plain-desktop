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
    let token = crate::base64_encode(&[1; 32]);
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    let server =
        super::super::ContentServer::start(&dir.path().join("plain.db"), &token, prefs).unwrap();
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
        crate::db::chat_store::messages::all(&state.db)
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
        crate::chat::events::WS_MESSAGE_CREATED
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
        crate::chat::events::WS_MESSAGE_UPDATED
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
        crate::db::chat_store::messages::get(&state.db, &id)
            .unwrap()
            .is_none()
    );
    let event = events.recv().await.unwrap();
    assert_eq!(event.event_type, crate::chat::events::WS_MESSAGE_DELETED);
    assert_eq!(
        serde_json::from_str::<Value>(&event.payload).unwrap(),
        json!("peer:local")
    );
}
