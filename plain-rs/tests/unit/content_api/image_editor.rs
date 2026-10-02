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
    assert_eq!(event.event_type, 34);
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
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let dir = tempfile::tempdir().unwrap();
    let token = crate::base64_encode(&[13; 32]);
    let server = ContentServer::start(
        &dir.path().join("plain.db"),
        &token,
        Arc::new(crate::prefs::Prefs::load(&dir.path().join("system_prefs.json")).unwrap()),
    )
    .unwrap();
    let mut socket = tokio::net::TcpStream::connect(("127.0.0.1", server.port))
        .await
        .unwrap();
    socket.write_all(format!("GET /events HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {token}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n").as_bytes()).await.unwrap();
    let mut headers = vec![];
    loop {
        headers.push(socket.read_u8().await.unwrap());
        if headers.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    async fn frame(socket: &mut tokio::net::TcpStream) -> (u8, Vec<u8>) {
        let code = socket.read_u8().await.unwrap();
        let size = socket.read_u8().await.unwrap();
        assert!(size < 126);
        let mut bytes = vec![0; size as usize];
        socket.read_exact(&mut bytes).await.unwrap();
        (code, bytes)
    }
    assert_eq!(frame(&mut socket).await.0, 0x81);
    let result = call(
        server.port,
        &token,
        r#"mutation {broadcastImageEditorUpdate(id:"p",update:"AQID")}"#,
    )
    .await;
    assert!(result.get("errors").is_none(), "{result}");
    let (code, bytes) = tokio::time::timeout(std::time::Duration::from_secs(2), frame(&mut socket))
        .await
        .unwrap();
    assert_eq!(code, 0x82);
    assert_eq!(bytes, vec![34, 0, 0, 0, 1, b'p', 1, 2, 3]);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(100), frame(&mut socket))
            .await
            .is_err()
    );
    drop(socket);
    server.shutdown().await;
}
