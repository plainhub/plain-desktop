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
