use super::*;

#[tokio::test]
async fn projects_persist_project_summaries_and_validate_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plain.db");
    let token = crate::base64_encode(&[12; 32]);
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("system_prefs.json")).unwrap());
    let server = ContentServer::start(&path, &token, prefs.clone()).unwrap();
    let save = r#"mutation {saveImageEditorProject(id:"",input:{stateB64:"AQID",thumbnail:"data:image/jpeg;base64,AQID",canvasWidth:100,canvasHeight:200,layerCount:2}) {id stateB64 thumbnail createdAt updatedAt}}"#;
    let result = call(server.port, &token, save).await;
    assert!(result.get("errors").is_none(), "{result}");
    let saved = &result["data"]["saveImageEditorProject"];
    let id = saved["id"].as_str().unwrap();
    assert!(!id.is_empty());
    let update = format!(
        r#"mutation {{saveImageEditorProject(id:"{id}",input:{{stateB64:"BAU",canvasWidth:300,canvasHeight:200,layerCount:3}}) {{id stateB64 createdAt}}}}"#
    );
    let updated = call(server.port, &token, &update).await;
    assert_eq!(
        updated["data"]["saveImageEditorProject"]["createdAt"],
        saved["createdAt"]
    );
    assert_eq!(updated["data"]["saveImageEditorProject"]["stateB64"], "BAU");
    for (state, width) in [("!", 100), ("AQID", -1)] {
        let invalid = format!(
            r#"mutation {{saveImageEditorProject(id:"invalid",input:{{stateB64:"{state}",canvasWidth:{width},canvasHeight:1,layerCount:1}}) {{id}}}}"#
        );
        assert!(
            call(server.port, &token, &invalid)
                .await
                .get("errors")
                .is_some()
        );
    }
    for n in 0..24 {
        let save = format!(
            r#"mutation {{saveImageEditorProject(id:"project-{n}%_",input:{{stateB64:"AQID",canvasWidth:1,canvasHeight:1,layerCount:1}}) {{id}}}}"#
        );
        assert!(
            call(server.port, &token, &save)
                .await
                .get("errors")
                .is_none()
        );
    }
    let list = call(
        server.port,
        &token,
        r#"{imageEditorProjects {id} imageEditorProjectItems(offset:0,limit:50,query:"%_") {id}}"#,
    )
    .await;
    assert_eq!(
        list["data"]["imageEditorProjects"]
            .as_array()
            .unwrap()
            .len(),
        20
    );
    assert_eq!(
        list["data"]["imageEditorProjectItems"]
            .as_array()
            .unwrap()
            .len(),
        24
    );
    let list = call(
        server.port,
        &token,
        r#"{imageEditorProjectItems(offset:20,limit:50,query:"%_") {id}}"#,
    )
    .await;
    assert_eq!(
        list["data"]["imageEditorProjectItems"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    server.shutdown().await;
    let server = ContentServer::start(&path, &token, prefs).unwrap();
    let read = call(
        server.port,
        &token,
        &format!(r#"{{imageEditorProject(id:"{id}") {{id stateB64 canvasWidth}}}}"#),
    )
    .await;
    assert_eq!(read["data"]["imageEditorProject"]["stateB64"], "BAU");
    assert_eq!(read["data"]["imageEditorProject"]["canvasWidth"], 300);
    call(
        server.port,
        &token,
        &format!(r#"mutation {{deleteImageEditorProject(id:"{id}")}}"#),
    )
    .await;
    let read = call(
        server.port,
        &token,
        &format!(r#"{{imageEditorProject(id:"{id}") {{id}}}}"#),
    )
    .await;
    assert!(read["data"]["imageEditorProject"].is_null());
    server.shutdown().await;
}

#[tokio::test]
async fn binary_updates_have_bounded_queue_and_exact_payload() {
    let (sender, mut receiver) = broadcast::channel(64);
    let updates = crate::image_editor::Updates::new(sender);
    updates.publish("画布", "AQID").unwrap();
    let event = receiver.recv().await.unwrap();
    assert_eq!(event.event_type, "IMAGE_EDITOR_UPDATE");
    assert_eq!(
        event.binary_payload.unwrap(),
        [vec![6], "画布".as_bytes().to_vec(), vec![1, 2, 3]].concat()
    );
    for (id, state) in [("", "AQID"), ("p", "!"), (&"p".repeat(256), "AQID")] {
        assert!(updates.publish(id, state).is_err());
    }
    let data = crate::base64_encode(&vec![0; 8 * 1024 * 1024]);
    for _ in 0..3 {
        updates.publish("p", &data).unwrap();
    }
    assert!(updates.publish("p", &data).is_err());
    drop(receiver.recv().await.unwrap());
    updates.publish("p", &data).unwrap();
}

#[tokio::test]
async fn websocket_binary_delta_does_not_invalidate_content() {
    use futures_util::StreamExt;
    use tokio_tungstenite::{connect_async, tungstenite::client::IntoClientRequest};
    let dir = tempfile::tempdir().unwrap();
    let token = crate::base64_encode(&[13; 32]);
    let server = ContentServer::start(
        &dir.path().join("plain.db"),
        &token,
        Arc::new(crate::prefs::Prefs::load(&dir.path().join("system_prefs.json")).unwrap()),
    )
    .unwrap();
    let mut request = format!("ws://127.0.0.1:{}/events", server.port)
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    let (mut socket, _) = connect_async(request).await.unwrap();
    let greeting = socket.next().await.unwrap().unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(greeting.to_text().unwrap()).unwrap()["type"],
        "CONTENT_CHANGED"
    );
    let result = call(
        server.port,
        &token,
        r#"mutation {broadcastImageEditorUpdate(id:"p",update:"AQID")}"#,
    )
    .await;
    assert!(result.get("errors").is_none(), "{result}");
    let frame = tokio::time::timeout(std::time::Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(frame.is_binary());
    let bytes = frame.into_data();
    let (kind, payload) = crate::ws_frame::decode_raw(&bytes).unwrap();
    assert_eq!(kind, "IMAGE_EDITOR_UPDATE");
    assert_eq!(payload, [1, b'p', 1, 2, 3]);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), socket.next())
            .await
            .is_err()
    );
    drop(socket);
    server.shutdown().await;
}
