//! What the desktop shell's own requests do against the public router.
//!
//! The shell sends its own `client_id` in the `c-id` header and no bearer
//! token (`src/lib/api/api.ts`), so the public `/graphql` has to admit that
//! client without any prior login. This exercises exactly that request.

use super::*;

/// A fresh core has never seen a login, yet the shell talks to it as itself.
/// If the public router needs a session row the shell never creates, every
/// local GraphQL call is a 401 and the app is dead on first paint.
#[tokio::test]
async fn the_shells_own_request_is_answered_without_any_login() {
    use plain_rs::db::Db;
    let dir = tempfile::tempdir().unwrap();
    let prefs =
        std::sync::Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    let core = crate::assembly::server::ContentServer::open(dir.path()).unwrap();
    let client_id = core.native_graphql().unwrap().client_id().to_owned();
    assert!(!client_id.is_empty(), "the shell needs an identity");

    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let (http, _) = core
        .start_public(
            0,
            0,
            cert.cert.pem().into_bytes(),
            cert.key_pair.serialize_pem().into_bytes(),
        )
        .await
        .unwrap();
    let state = core.runtime_state();
    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{http}/graphql"))
        .header("content-type", "application/json")
        .header("c-id", &client_id)
        .body(serde_json::json!({"query":"{ __typename }"}).to_string())
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(
        status.as_u16(),
        200,
        "the shell's own GraphQL call was refused: {body}"
    );
    assert!(body.contains("Query"), "{body}");
    let _ = (
        Db::open(std::path::Path::new(":memory:")).unwrap(),
        prefs,
        state,
    );
    core.stop_core().await;
}

/// The shell's event socket carries only `cid` and one chacha frame built
/// from the local URL token. It must be admitted, or the UI never hears
/// about anything again after a reload.
#[tokio::test]
async fn the_shells_event_socket_is_admitted_on_its_client_id() {
    let dir = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(dir.path()).unwrap();
    let client_id = core.native_graphql().unwrap().client_id().to_owned();
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let (http, _) = core
        .start_public(
            0,
            0,
            cert.cert.pem().into_bytes(),
            cert.key_pair.serialize_pem().into_bytes(),
        )
        .await
        .unwrap();
    let url = format!("ws://127.0.0.1:{http}/?cid={client_id}");
    let (mut socket, response) = tokio_tungstenite::connect_async(url)
        .await
        .expect("the shell socket must connect");
    assert_eq!(
        response.status().as_u16(),
        101,
        "the socket upgrade must be accepted"
    );
    // The shell encrypts its frames with the local URL token, exactly as
    // `app-socket.ts` does after the socket opens.
    use futures_util::SinkExt;
    let key = plain_rs::utils::base64::base64_decode(&core.native_url_token().unwrap());
    let frame = plain_rs::crypto::xchacha_encrypt_raw(&key, b"1700000000000").unwrap();
    socket
        .send(tokio_tungstenite::tungstenite::Message::Binary(
            frame.into(),
        ))
        .await
        .unwrap();
    // A frame the server could decrypt keeps the socket open; an unreadable
    // one closes it.
    let stayed_open = tokio::time::timeout(
        std::time::Duration::from_millis(200),
        futures_util::StreamExt::next(&mut socket),
    )
    .await;
    assert!(
        stayed_open.is_err(),
        "the socket was closed after a valid frame: {stayed_open:?}"
    );
    // Events must come back readable with the same local token. Checking
    // only that the socket stays open would miss a server that admits the
    // shell and then cannot encrypt anything it sends.
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    // Registration asks the platform to confirm, so the host has to answer
    // before the socket starts forwarding events.
    let host = core.runtime_state().host.clone();
    let (generation, mut requests) = host.connect();
    tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let _ = host.reply(
                generation,
                serde_json::json!({"id":request["id"],"result":true}),
            );
        }
    });
    core.runtime_state()
        .events
        .send(plain_rs::events::WsEvent::broadcast(
            "CHAT_MESSAGE",
            serde_json::json!({"probe":true}).to_string(),
        ))
        .expect("the core broadcasts");
    let outbound = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        futures_util::StreamExt::next(&mut socket),
    )
    .await
    .expect("the shell must receive events")
    .expect("a frame")
    .expect("a readable frame");
    let payload = match &outbound {
        tokio_tungstenite::tungstenite::Message::Binary(bytes) => bytes,
        other => panic!("expected a binary frame, got {other:?}"),
    };
    let (name, body) = plain_rs::protocols::websocket::decode(payload, &key)
        .expect("the outbound frame must decrypt with the local token");
    assert_eq!(name, "CHAT_MESSAGE");
    assert!(String::from_utf8_lossy(&body).contains("probe"), "{body:?}");
    core.stop_core().await;
}

/// Local file bytes come from an opaque native handle rather than `/fs`,
/// so opening one must work without any session on the core.
#[tokio::test]
async fn a_local_file_resolves_to_a_native_handle_without_a_session() {
    let dir = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(dir.path()).unwrap();
    let path = dir.path().join("note.txt");
    std::fs::write(&path, b"local bytes").unwrap();
    // Path resolution asks the platform where its own directories are; the
    // bootstrap supplies that through the host bridge.
    let state = core.runtime_state();
    let host = state.host.clone();
    let (generation, mut requests) = host.connect();
    let data_dir = dir.path().to_path_buf();
    tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let reply = if request["method"] == "fileTaskAuthorize" {
                serde_json::json!(true)
            } else if request["method"] == "systemFileResource" {
                match request["params"]["operation"].as_str().unwrap_or_default() {
                    "appDir" => serde_json::json!(data_dir.to_str().unwrap()),
                    "mime" => serde_json::json!("text/plain"),
                    _ => serde_json::Value::Null,
                }
            } else {
                serde_json::json!({})
            };
            let _ = host.reply(
                generation,
                serde_json::json!({"id":request["id"],"result":reply}),
            );
        }
    });
    let resolved = core
        .resolve_native_file(&path.to_string_lossy())
        .await
        .expect("the core resolves its own files without a session");
    assert!(resolved.is_absolute());
    assert_eq!(std::fs::read(&resolved).unwrap(), b"local bytes");
    core.stop_core().await;
}

/// Trashing a local file has to work against the core. The resolver asks for
/// the legacy media database by type, which only the API context injects —
/// so this is the shape that catches the shell losing the operation.
#[tokio::test]
async fn a_local_trash_reaches_the_operation_instead_of_failing_on_context() {
    let dir = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(dir.path()).unwrap();
    let state = core.runtime_state();
    let host = state.host.clone();
    let (generation, mut requests) = host.connect();
    let data_dir = dir.path().to_path_buf();
    tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let reply = match request["method"].as_str().unwrap_or_default() {
                "fileTaskAuthorize" => serde_json::json!(true),
                "fileTaskMediaSnapshot" => serde_json::json!([]),
                "systemFileResource" => serde_json::json!(data_dir.to_str().unwrap()),
                "mdnsMulticast" => serde_json::json!(true),
                "systemMediaIdsFacts" => serde_json::json!([]),
                other => panic!("unexpected host method {other}"),
            };
            let _ = host.reply(
                generation,
                serde_json::json!({"id":request["id"],"result":reply}),
            );
        }
    });
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let (http, _) = core
        .start_public(
            0,
            0,
            cert.cert.pem().into_bytes(),
            cert.key_pair.serialize_pem().into_bytes(),
        )
        .await
        .unwrap();
    let client_id = core.native_graphql().unwrap().client_id().to_owned();
    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{http}/graphql"))
        .header("content-type", "application/json")
        .header("c-id", &client_id)
        .body(serde_json::json!({
            "query": r#"mutation { trashMediaItems(type: IMAGE, query: "all:true") { affectedCount } }"#
        })
        .to_string())
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status.as_u16(), 200, "refused: {body}");
    assert!(
        !body.contains("Failed to resolve") && !body.contains("errors"),
        "the operation lost its context against the core: {body}"
    );
    assert!(body.contains("affectedCount"), "{body}");
    core.stop_core().await;
}

/// The media library is the largest surface the shell reads. Its resolvers
/// ask for the library databases by type, so the core context has to
/// supply them or every list in the UI comes back empty.
#[tokio::test]
async fn a_local_media_list_is_answered_by_the_core() {
    let dir = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(dir.path()).unwrap();
    let state = core.runtime_state();
    let host = state.host.clone();
    let (generation, mut requests) = host.connect();
    let data_dir = dir.path().to_path_buf();
    tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let reply = match request["method"].as_str().unwrap_or_default() {
                "fileTaskAuthorize" => serde_json::json!(true),
                "fileTaskMediaSnapshot" | "systemMediaIdsFacts" | "systemMediaRows" => {
                    serde_json::json!([])
                }
                "systemFileResource" => serde_json::json!(data_dir.to_str().unwrap()),
                other => panic!("unexpected host method {other}"),
            };
            let _ = host.reply(
                generation,
                serde_json::json!({"id":request["id"],"result":reply}),
            );
        }
    });
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let (http, _) = core
        .start_public(
            0,
            0,
            cert.cert.pem().into_bytes(),
            cert.key_pair.serialize_pem().into_bytes(),
        )
        .await
        .unwrap();
    let client_id = core.native_graphql().unwrap().client_id().to_owned();
    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{http}/graphql"))
        .header("content-type", "application/json")
        .header("c-id", &client_id)
        .body(serde_json::json!({
            "query": r#"{ audios(query: "", offset: 0, limit: 5, sortBy: DATE_DESC) { id path title } }"#
        })
        .to_string())
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status.as_u16(), 200, "refused: {body}");
    assert!(!body.contains("errors"), "{body}");
    assert!(body.contains("\"audios\""), "{body}");
    core.stop_core().await;
}

/// `/init` hands the shell its signature key, and decrypts the request with
/// the same local token as everything else. Without that the shell has no key
/// to talk to peers with.
#[tokio::test]
async fn init_answers_the_shells_encrypted_request() {
    let dir = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(dir.path()).unwrap();
    let state = core.runtime_state();
    let host = state.host.clone();
    let (generation, mut requests) = host.connect();
    let data_dir = dir.path().to_path_buf();
    tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let reply = match request["method"].as_str().unwrap_or_default() {
                "systemFileResource" => serde_json::json!(data_dir.to_str().unwrap()),
                other => panic!("unexpected host method {other}"),
            };
            let _ = host.reply(
                generation,
                serde_json::json!({"id":request["id"],"result":reply}),
            );
        }
    });
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let (http, _) = core
        .start_public(
            0,
            0,
            cert.cert.pem().into_bytes(),
            cert.key_pair.serialize_pem().into_bytes(),
        )
        .await
        .unwrap();
    let client_id = core.native_graphql().unwrap().client_id().to_owned();
    let key = plain_rs::utils::base64::base64_decode(&core.native_url_token().unwrap());
    let body = plain_rs::crypto::xchacha_encrypt_raw(&key, b"{}").unwrap();
    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{http}/init"))
        .header("content-type", "application/json")
        .header("c-id", &client_id)
        .body(body)
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status.as_u16(), 200, "refused: {body}");
    let value: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(
        value["signaturePublicKey"]
            .as_str()
            .is_some_and(|key| !key.is_empty()),
        "the shell needs a signature key: {body}"
    );
    core.stop_core().await;
}

/// The shell lists its upload chunks over GraphQL (the bytes themselves go
/// through the native writer). That listing must not need a session.
#[tokio::test]
async fn a_local_upload_chunk_listing_is_answered_by_the_core() {
    let dir = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(dir.path()).unwrap();
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let (http, _) = core
        .start_public(
            0,
            0,
            cert.cert.pem().into_bytes(),
            cert.key_pair.serialize_pem().into_bytes(),
        )
        .await
        .unwrap();
    let client_id = core.native_graphql().unwrap().client_id().to_owned();
    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{http}/graphql"))
        .header("content-type", "application/json")
        .header("c-id", &client_id)
        .body(
            serde_json::json!({
                "query": r#"{ uploadedChunks(fileId: "local-upload") }"#
            })
            .to_string(),
        )
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(status.as_u16(), 200, "refused: {body}");
    assert!(!body.contains("errors"), "{body}");
    assert!(body.contains("uploadedChunks"), "{body}");
    core.stop_core().await;
}

/// The shell drives the DLNA receiver through the core, and the bootstrap
/// enables it the same way. Enabling needs a live public port, so the core
/// must refuse rather than half-start when there is none.
#[tokio::test]
async fn dlna_state_follows_the_public_listener() {
    let dir = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(dir.path()).unwrap();
    let dlna = core.native_dlna().unwrap();
    assert_eq!(
        dlna.snapshot().await.unwrap()["isRunning"],
        false,
        "a core without a listener does not run a receiver"
    );
    assert!(
        dlna.set_enabled(true).await.is_err(),
        "enabling without a listener has no port to advertise"
    );

    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let (http, _) = core
        .start_public(
            0,
            0,
            cert.cert.pem().into_bytes(),
            cert.key_pair.serialize_pem().into_bytes(),
        )
        .await
        .unwrap();
    let host = core.runtime_state().host.clone();
    let (generation, mut requests) = host.connect();
    tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let reply = serde_json::json!(request["params"]["acquire"].as_bool().unwrap_or(true));
            let _ = host.reply(
                generation,
                serde_json::json!({"id":request["id"],"result":reply}),
            );
        }
    });
    dlna.set_enabled(true).await.unwrap();
    let snapshot = dlna.snapshot().await.unwrap();
    assert_eq!(snapshot["enabled"], true);
    assert_eq!(
        snapshot["port"], http,
        "the receiver advertises the listener's port"
    );
    dlna.set_enabled(false).await.unwrap();
    assert_eq!(dlna.snapshot().await.unwrap()["enabled"], false);
    core.stop_core().await;
}
