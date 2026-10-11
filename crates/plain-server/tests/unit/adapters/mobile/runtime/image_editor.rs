use super::*;

#[tokio::test]
async fn projects_persist_project_summaries_and_validate_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plain.db");
    let token = plain_rs::utils::base64::base64_encode(&[12; 32]);
    let prefs =
        Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("system_prefs.json")).unwrap());
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
    let updates = plain_rs::media::images::editor::Updates::new(sender);
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
    let data = plain_rs::utils::base64::base64_encode(&vec![0; 8 * 1024 * 1024]);
    for _ in 0..3 {
        updates.publish("p", &data).unwrap();
    }
    assert!(updates.publish("p", &data).is_err());
    drop(receiver.recv().await.unwrap());
    updates.publish("p", &data).unwrap();
}

#[tokio::test]
async fn binary_delta_does_not_invalidate_content() {
    let dir = tempfile::tempdir().unwrap();
    let token = plain_rs::utils::base64::base64_encode(&[13; 32]);
    let server = ContentServer::start(
        &dir.path().join("plain.db"),
        &token,
        Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("system_prefs.json")).unwrap()),
    )
    .unwrap();
    let mut socket = server.runtime_state().events.subscribe();
    let result = call(
        server.port,
        &token,
        r#"mutation {broadcastImageEditorUpdate(id:"p",update:"AQID")}"#,
    )
    .await;
    assert!(result.get("errors").is_none(), "{result}");
    let event = tokio::time::timeout(std::time::Duration::from_secs(2), socket.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event.event_type, "IMAGE_EDITOR_UPDATE");
    assert_eq!(event.binary_payload.unwrap(), [1, b'p', 1, 2, 3]);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), socket.recv())
            .await
            .is_err()
    );
    drop(socket);
    server.shutdown().await;
}
