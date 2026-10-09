//! Unit tests for the nas `/` root handler and the login-handshake
//! crypto chain (moved from plain-nas).
use crate::http_server::build_router;
use crate::http_server::test_support::nas_state;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::response::Response;
use tower::ServiceExt;

fn ws_upgrade_request(uri: &str) -> Request<Body> {
    Request::get(uri)
        .header(header::CONNECTION, "Upgrade")
        .header(header::UPGRADE, "websocket")
        .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
        .header("sec-websocket-version", "13")
        .body(Body::empty())
        .unwrap()
}

async fn body_text(resp: Response) -> String {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body reads");
    String::from_utf8(bytes.to_vec()).expect("utf-8 body")
}

/// plain-app contract: the main WS channel lives at `/`. A browser hitting
/// `/` (plain navigation, no upgrade headers) gets the SPA index — never a
/// 400 from a missing `cid`.
#[tokio::test]
async fn root_serves_index_for_plain_get() {
    let app = build_router(nas_state());
    let resp = app
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        resp.headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .starts_with("text/html")
    );
}

/// The 101 upgrade path needs hyper's `OnUpgrade` extension, which only a
/// real server injects — spawn one on an ephemeral loopback port.
async fn spawn_server() -> std::net::SocketAddr {
    let app = build_router(nas_state());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

/// Send a raw HTTP request, read until the end of the response headers.
async fn raw_http(addr: std::net::SocketAddr, request: &str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let n = stream.read(&mut chunk).await.unwrap();
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

const UPGRADE_REQUEST: &str = "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n";

/// plain-app contract: the main WS channel lives at `/`.
#[tokio::test]
async fn root_upgrades_websocket() {
    let addr = spawn_server().await;
    let resp = raw_http(
        addr,
        &UPGRADE_REQUEST.replace("{path}", "/?cid=test-client"),
    )
    .await;
    assert!(resp.starts_with("HTTP/1.1 101"), "got: {resp}");
}

/// Upgrade without `cid` is rejected with 400 before the socket opens.
#[tokio::test]
async fn root_rejects_upgrade_without_cid() {
    let addr = spawn_server().await;
    let resp = raw_http(addr, &UPGRADE_REQUEST.replace("{path}", "/")).await;
    assert!(resp.starts_with("HTTP/1.1 400"), "got: {resp}");
}

/// `/ws` is no longer a WebSocket endpoint (aligned with plain-app): an
/// upgrade attempt falls through to the SPA fallback and gets index.html —
/// the same HTTP 200 a plain-app client would see on an unknown path.
#[tokio::test]
async fn legacy_ws_path_is_spa_not_socket() {
    let app = build_router(nas_state());
    let resp = app
        .oneshot(ws_upgrade_request("/ws?cid=test-client"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(body_text(resp).await.contains("<!doctype html>"));
}

/// The full server-side crypto chain of the login handshake: password-key
/// derivation, frame decryption, ECDH token agreement and the Ed25519
/// signature over the exact response string the client verifies.
#[test]
fn auth_handshake_crypto_chain() {
    let hash = "ab".repeat(64); // sha512 hex as stored by PasswordStore
    let key: [u8; 32] = hash.as_bytes()[..32].try_into().unwrap();

    // Client frame: server and client each hold an ECDH session.
    let server = crate::crypto::EcdhSession::generate();
    let client = crate::crypto::EcdhSession::generate();
    let client_pub_b64 = crate::utils::base64::base64_encode(&client.public_key_bytes);
    let req = serde_json::json!({
        "password": hash,
        "ecdhPublicKey": client_pub_b64,
    });
    let frame = crate::xchacha_encrypt_raw(&key, req.to_string().as_bytes()).unwrap();

    // Server side: decrypt + parse.
    let plaintext = crate::xchacha_decrypt_raw(&key, &frame).expect("decrypts with password key");
    let req: serde_json::Value = serde_json::from_slice(&plaintext).unwrap();
    assert_eq!(req["password"].as_str().unwrap(), hash);

    // Both sides derive the same session token.
    let client_pub =
        crate::utils::base64::base64_decode_checked(req["ecdhPublicKey"].as_str().unwrap())
            .unwrap();
    let server_pub_b64 = crate::utils::base64::base64_encode(&server.public_key_bytes);
    let client_token = client.compute_shared_key(&server.public_key_bytes).unwrap();
    let server_token = server.compute_shared_key(&client_pub).unwrap();
    assert_eq!(
        server_token, client_token,
        "both sides derive the same token"
    );

    // Signature over the exact response string, verified with the /init key.
    let (keypair, public) = crate::ed25519_generate();
    let public_b64 = crate::utils::base64::base64_encode(&public);
    let ts: u64 = 1_700_000_000_000;
    let msg = format!("client-1|OK|{server_pub_b64}|{ts}");
    let sig = crate::ed25519_sign(&keypair, msg.as_bytes());
    assert!(crate::ed25519_verify(&public_b64, msg.as_bytes(), &sig));
    assert!(!crate::ed25519_verify(&public_b64, b"tampered", &sig));
}
