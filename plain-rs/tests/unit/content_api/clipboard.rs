use super::*;

#[tokio::test]
async fn http_record_search_delete_and_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plain.db");
    let token = crate::base64_encode(&[9; 32]);
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("system_prefs.json")).unwrap());
    let server = ContentServer::start(&path, &token, prefs.clone()).unwrap();
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
    assert!(
        String::from_utf8(headers)
            .unwrap()
            .starts_with("HTTP/1.1 101")
    );
    async fn event(socket: &mut tokio::net::TcpStream) -> serde_json::Value {
        use tokio::io::AsyncReadExt;
        let mut header = [0; 2];
        socket.read_exact(&mut header).await.unwrap();
        assert_eq!(header[0], 0x81);
        assert!(header[1] < 126);
        let mut bytes = vec![0; header[1] as usize];
        socket.read_exact(&mut bytes).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }
    assert_eq!(event(&mut socket).await["type"], 47);
    let query = r#"mutation {recordClipboard(input:{text:"100%_ 🌍",source:"peer",label:"label",sensitive:true}) {inserted item {id text source label sensitive createdAt}}}"#;
    let first = call(server.port, &token, query).await;
    assert!(first.get("errors").is_none(), "{first}");
    assert_eq!(first["data"]["recordClipboard"]["inserted"], true);
    let changed = tokio::time::timeout(std::time::Duration::from_secs(2), event(&mut socket))
        .await
        .unwrap();
    assert_eq!(changed["type"], 47);
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
