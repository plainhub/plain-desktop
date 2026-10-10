use super::*;

#[tokio::test]
async fn http_record_search_delete_and_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plain.db");
    let token = crate::base64_encode(&[9; 32]);
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("system_prefs.json")).unwrap());
    let server = ContentServer::start(&path, &token, prefs.clone()).unwrap();
    use futures_util::StreamExt;
    use tokio_tungstenite::{connect_async, tungstenite::client::IntoClientRequest};
    let mut request = format!("ws://127.0.0.1:{}/events", server.port)
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    let (mut socket, _) = connect_async(request).await.unwrap();
    async fn event(
        socket: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    ) -> serde_json::Value {
        let frame = socket.next().await.unwrap().unwrap();
        serde_json::from_str(frame.to_text().unwrap()).unwrap()
    }
    assert_eq!(event(&mut socket).await["type"], "CONTENT_CHANGED");
    let query = r#"mutation {recordClipboard(input:{text:"100%_ 🌍",source:"peer",label:"label",sensitive:true}) {inserted item {id text source label sensitive createdAt}}}"#;
    let first = call(server.port, &token, query).await;
    assert!(first.get("errors").is_none(), "{first}");
    assert_eq!(first["data"]["recordClipboard"]["inserted"], true);
    let changed = tokio::time::timeout(std::time::Duration::from_secs(2), event(&mut socket))
        .await
        .unwrap();
    assert_eq!(changed["type"], "CONTENT_CHANGED");
    assert_eq!(changed["payload"], "{}");
    drop(socket);

    let item = &first["data"]["recordClipboard"]["item"];
    assert_eq!(item["source"], "peer");
    assert_eq!(item["sensitive"], true);
    assert!(chrono::DateTime::parse_from_rfc3339(item["createdAt"].as_str().unwrap()).is_ok());
    assert_eq!(
        call(server.port, &token, query).await["data"]["recordClipboard"]["inserted"],
        false
    );
    let page = call(
        server.port,
        &token,
        r#"{clipboardItems(offset:0,limit:50,query:"%_") {id} clipboardItemCount(query:"%_")}"#,
    )
    .await;
    assert_eq!(page["data"]["clipboardItemCount"], 1);
    assert_eq!(page["data"]["clipboardItems"][0]["id"], item["id"]);
    for invalid in [
        r#"mutation {recordClipboard(input:{text:" ",source:"",label:"",sensitive:false}) {inserted}}"#,
        r#"mutation {deleteClipboardItems(query:" ") {affectedCount}}"#,
        r#"mutation {deleteClipboardItems(query:"source:peer") {affectedCount}}"#,
    ] {
        assert!(
            call(server.port, &token, invalid)
                .await
                .get("errors")
                .is_some()
        );
    }
    assert_eq!(
        call(
            server.port,
            &token,
            "mutation {deleteClipboardItemsByIds(ids:[]) {affectedCount}}"
        )
        .await["data"]["deleteClipboardItemsByIds"]["affectedCount"],
        0
    );
    server.shutdown().await;
    let server = ContentServer::start(&path, &token, prefs).unwrap();
    assert_eq!(
        call(server.port, &token, r#"{clipboardItemCount(query:"")}"#).await["data"]["clipboardItemCount"],
        1
    );
    assert_eq!(
        call(
            server.port,
            &token,
            r#"mutation {deleteClipboardItems(query:"all:true") {affectedCount}}"#
        )
        .await["data"]["deleteClipboardItems"]["affectedCount"],
        1
    );
    server.shutdown().await;
}

#[test]
fn clipboard_wire_contract() {
    let db = Arc::new(Db::open(std::path::Path::new(":memory:")).unwrap());
    let dir = tempfile::tempdir().unwrap();
    let (events, _) = broadcast::channel(16);
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("system_prefs.json")).unwrap());
    let sdl = schema::build(db, events, prefs, dir.path().into()).sdl();
    for field in [
        "clipboardItems(offset: Int!, limit: Int!, query: String!): [ClipboardItem!]!",
        "clipboardItemCount(query: String!): Int!",
        "deleteClipboardItems(query: String!): ActionResult!",
        "recordClipboard(input: ClipboardRecordInput!): ClipboardRecordResult!",
    ] {
        assert!(sdl.contains(field), "missing {field}");
    }
    assert!(sdl.contains("createdAt: Instant!"));
    if std::env::var_os("UPDATE_CONTENT_SCHEMA").is_some() {
        std::fs::write(
            concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/content-api.graphqls"),
            &sdl,
        )
        .unwrap();
    } else {
        assert_eq!(sdl, include_str!("../../../testdata/content-api.graphqls"));
    }
}
