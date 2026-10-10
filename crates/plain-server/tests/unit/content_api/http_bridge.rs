use super::*;
use crate::content_api::ContentServer;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message as WsMessage, client::IntoClientRequest},
};

async fn setup() -> (tempfile::TempDir, ContentServer, String, u16, u16) {
    let dir = tempfile::tempdir().unwrap();
    let token = crate::base64_encode(&[61; 32]);
    let server = ContentServer::start(
        &dir.path().join("plain.db"),
        &token,
        Arc::new(crate::prefs::Prefs::load(&dir.path().join("system.json")).unwrap()),
    )
    .unwrap();
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let (http, https) = server
        .start_public(
            0,
            0,
            cert.cert.pem().into_bytes(),
            cert.key_pair.serialize_pem().into_bytes(),
        )
        .await
        .unwrap();
    (dir, server, token, http, https)
}
async fn socket(
    port: u16,
    token: &str,
    path: &str,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let mut request = format!("ws://127.0.0.1:{port}{path}")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    connect_async(request).await.unwrap().0
}
#[tokio::test]
async fn public_streams_multi_chunk_body_and_preserves_peer_uri_headers() {
    let (_dir, server, token, http, https) = setup().await;
    let mut host = socket(server.port, &token, "/host").await;
    let port = server.port;
    let task = tokio::spawn(async move {
        use futures_util::SinkExt;
        for _ in 0..2 {
            let request: Value =
                serde_json::from_str(host.next().await.unwrap().unwrap().to_text().unwrap())
                    .unwrap();
            assert_eq!(request["method"], "httpExchange");
            host.send(WsMessage::Text(
                json!({"id":request["id"],"result":null}).to_string().into(),
            ))
            .await
            .unwrap();
            let mut stream = socket(
                port,
                &token,
                &format!("/http_host/{}", request["params"]["id"].as_str().unwrap()),
            )
            .await;
            let metadata: Value =
                serde_json::from_str(stream.next().await.unwrap().unwrap().to_text().unwrap())
                    .unwrap();
            assert_eq!(metadata["remoteHost"], "127.0.0.1");
            assert!(matches!(
                metadata["scheme"].as_str(),
                Some("http" | "https")
            ));
            assert_eq!(metadata["uri"], "/echo?path=a%20b");
            let mut body = Vec::new();
            loop {
                match stream.next().await.unwrap().unwrap() {
                    WsMessage::Binary(bytes) => {
                        assert!(bytes.len() <= CHUNK);
                        body.extend_from_slice(&bytes);
                    }
                    WsMessage::Text(text) => {
                        let packet: Value = serde_json::from_str(&text).unwrap();
                        if packet["kind"] == "end" {
                            break;
                        }
                    }
                    _ => {}
                }
            }
            assert_eq!(body, vec![23; CHUNK * 5 + 7]);
            stream.send(WsMessage::Text(json!({"kind":"response","status":201,"headers":{"x-test":["stream"],"content-length":[body.len().to_string()]}}).to_string().into())).await.unwrap();
            for chunk in body.chunks(CHUNK) {
                stream
                    .send(WsMessage::Binary(chunk.to_vec().into()))
                    .await
                    .unwrap();
            }
            stream
                .send(WsMessage::Text(json!({"kind":"end"}).to_string().into()))
                .await
                .unwrap();
        }
    });
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();
    for (scheme, port) in [("http", http), ("https", https)] {
        let response = client
            .post(format!("{scheme}://127.0.0.1:{port}/echo?path=a%20b"))
            .body(vec![23; CHUNK * 5 + 7])
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 201);
        assert_eq!(response.headers()["x-test"], "stream");
        assert_eq!(
            response.bytes().await.unwrap().as_ref(),
            vec![23; CHUNK * 5 + 7].as_slice()
        );
    }
    task.await.unwrap();
    server.shutdown().await;
}
#[tokio::test]
async fn internal_exchange_rejects_missing_auth_and_unknown_id() {
    let (_dir, server, token, _http, _https) = setup().await;
    let url = format!("ws://127.0.0.1:{}/http_host/unknown", server.port);
    assert!(connect_async(&url).await.is_err());
    let mut stream = socket(server.port, &token, "/http_host/unknown").await;
    assert!(matches!(
        stream.next().await,
        Some(Ok(WsMessage::Close(_))) | None
    ));
    server.shutdown().await;
}

#[tokio::test]
async fn multipart_preserves_parts_and_streams_large_file() {
    use futures_util::SinkExt;
    let (_dir, server, token, http, _) = setup().await;
    let mut host = socket(server.port, &token, "/host").await;
    let port = server.port;
    let task = tokio::spawn(async move {
        let request: Value =
            serde_json::from_str(host.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        host.send(WsMessage::Text(
            json!({"id":request["id"],"result":null}).to_string().into(),
        ))
        .await
        .unwrap();
        let mut stream = socket(
            port,
            &token,
            &format!("/http_host/{}", request["params"]["id"].as_str().unwrap()),
        )
        .await;
        stream.next().await.unwrap().unwrap();
        let mut parts = Vec::new();
        let mut body = Vec::new();
        loop {
            match stream.next().await.unwrap().unwrap() {
                WsMessage::Binary(bytes) => {
                    assert!(bytes.len() <= CHUNK);
                    body.extend_from_slice(&bytes);
                }
                WsMessage::Text(text) => {
                    let packet: Value = serde_json::from_str(&text).unwrap();
                    match packet["kind"].as_str().unwrap() {
                        "part" => {
                            assert!(body.is_empty());
                            parts.push(packet);
                        }
                        "partEnd" => {
                            if parts.len() == 1 {
                                assert_eq!(body, b"metadata");
                            } else {
                                assert_eq!(body, vec![b'z'; CHUNK * 4 + 1]);
                            }
                            body.clear();
                        }
                        "end" => break,
                        other => panic!("{other}"),
                    }
                }
                _ => {}
            }
        }
        assert_eq!(parts[0]["name"], "info");
        assert_eq!(parts[1]["filename"], "sample.bin");
        assert_eq!(parts[1]["contentType"], "application/octet-stream");
        stream
            .send(WsMessage::Text(
                json!({"kind":"response","status":204,"headers":{"content-length":["0"]}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        stream
            .send(WsMessage::Text(json!({"kind":"end"}).to_string().into()))
            .await
            .unwrap();
    });
    let mut body=b"--test\r\nContent-Disposition: form-data; name=\"info\"\r\n\r\nmetadata\r\n--test\r\nContent-Disposition: form-data; name=\"file\"; filename=\"sample.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n".to_vec();
    body.extend(vec![b'z'; CHUNK * 4 + 1]);
    body.extend_from_slice(b"\r\n--test--\r\n");
    let response = reqwest::Client::new()
        // A path the Rust router does not claim, so this still exercises the
        // bridge multipart forwarding for the routes that have not moved yet.
        .post(format!("http://127.0.0.1:{http}/multipart-probe"))
        .header("content-type", "multipart/form-data; boundary=test")
        .body(body)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);
    task.await.unwrap();
    server.shutdown().await;
}

#[tokio::test]
async fn websocket_upgrade_denial_binary_text_and_shutdown_are_preserved() {
    use futures_util::SinkExt;
    let (_dir, server, token, http, _) = setup().await;
    let mut host = socket(server.port, &token, "/host").await;
    let port = server.port;
    let task = tokio::spawn(async move {
        for index in 0..2 {
            let request: Value =
                serde_json::from_str(host.next().await.unwrap().unwrap().to_text().unwrap())
                    .unwrap();
            host.send(WsMessage::Text(
                json!({"id":request["id"],"result":null}).to_string().into(),
            ))
            .await
            .unwrap();
            let mut stream = socket(
                port,
                &token,
                &format!("/http_host/{}", request["params"]["id"].as_str().unwrap()),
            )
            .await;
            let packet: Value =
                serde_json::from_str(stream.next().await.unwrap().unwrap().to_text().unwrap())
                    .unwrap();
            assert_eq!(packet["webSocket"], true);
            if index == 0 {
                stream
                    .send(WsMessage::Text(
                        json!({"kind":"response","status":403,"headers":{"content-length":["0"]}})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
                stream
                    .send(WsMessage::Text(json!({"kind":"end"}).to_string().into()))
                    .await
                    .unwrap();
                continue;
            }
            stream
                .send(WsMessage::Text(
                    json!({"kind":"upgrade","status":101,"headers":{}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
            let mut binary = false;
            let mut text = false;
            while !binary || !text {
                match stream.next().await.unwrap().unwrap() {
                    WsMessage::Binary(bytes) => {
                        assert_eq!(&bytes[..], &[1, 2, 3]);
                        binary = true;
                        stream.send(WsMessage::Binary(bytes)).await.unwrap();
                    }
                    WsMessage::Text(value) => {
                        let packet: Value = serde_json::from_str(&value).unwrap();
                        if packet["kind"] == "wsText" {
                            assert_eq!(packet["text"], "hello");
                            text = true;
                            stream.send(WsMessage::Text(value)).await.unwrap();
                        }
                    }
                    _ => {}
                }
            }
            assert!(matches!(
                stream.next().await,
                Some(Ok(WsMessage::Close(_))) | None
            ));
        }
    });
    assert!(
        connect_async(format!("ws://127.0.0.1:{http}/denied"))
            .await
            .is_err()
    );
    let (mut external, _) = connect_async(format!("ws://127.0.0.1:{http}/bridge-test"))
        .await
        .unwrap();
    external
        .send(WsMessage::Binary(vec![1, 2, 3].into()))
        .await
        .unwrap();
    external
        .send(WsMessage::Text("hello".into()))
        .await
        .unwrap();
    assert_eq!(
        external.next().await.unwrap().unwrap(),
        WsMessage::Binary(vec![1, 2, 3].into())
    );
    assert_eq!(
        external.next().await.unwrap().unwrap(),
        WsMessage::Text("hello".into())
    );
    tokio::time::timeout(Duration::from_secs(3), server.stop_public())
        .await
        .unwrap();
    assert!(matches!(
        external.next().await,
        Some(Ok(WsMessage::Close(_))) | None | Some(Err(_))
    ));
    task.await.unwrap();
    server.shutdown().await;
}

#[tokio::test]
async fn host_authorized_file_uses_range_and_does_not_buffer_whole_file() {
    use futures_util::SinkExt;
    let (dir, server, token, http, _) = setup().await;
    let path = dir.path().join("file.bin");
    std::fs::write(&path, vec![41; CHUNK * 20]).unwrap();
    let mut host = socket(server.port, &token, "/host").await;
    let port = server.port;
    let task = tokio::spawn(async move {
        let request: Value =
            serde_json::from_str(host.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        host.send(WsMessage::Text(
            json!({"id":request["id"],"result":null}).to_string().into(),
        ))
        .await
        .unwrap();
        let mut stream = socket(
            port,
            &token,
            &format!("/http_host/{}", request["params"]["id"].as_str().unwrap()),
        )
        .await;
        stream.next().await.unwrap().unwrap();
        stream.send(WsMessage::Text(json!({"kind":"file","path":path,"contentType":"application/octet-stream","headers":{"content-disposition":["attachment; filename=file.bin"]}}).to_string().into())).await.unwrap();
    });
    let response = reqwest::Client::new()
        .get(format!("http://127.0.0.1:{http}/fs?id=opaque"))
        .header("range", "bytes=10-30")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 206);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["content-length"], "21");
    assert_eq!(
        response.headers()["content-range"],
        format!("bytes 10-30/{}", CHUNK * 20)
    );
    assert_eq!(response.bytes().await.unwrap().as_ref(), [41; 21]);
    task.await.unwrap();
    server.shutdown().await;
}
