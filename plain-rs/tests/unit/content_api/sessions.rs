use super::*;
use crate::content_api::{ContentServer, main_graphql, ws_login};
use std::sync::Arc;

fn fixture() -> (tempfile::TempDir, ContentServer) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(crate::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap());
    prefs.set_user("service", true).unwrap();
    prefs.set_user("desktop_access", true).unwrap();
    prefs.set("auth_two_factor", false).unwrap();
    prefs.set("password", "secret").unwrap();
    prefs.set("client_id", "phone").unwrap();
    let (private, public) = crate::ed25519_generate();
    prefs.set("signature_key_pair", json!({"privateKey":crate::base64_encode(&private[..32]),"publicKey":crate::base64_encode(&public)}).to_string()).unwrap();
    let server = ContentServer::start(
        &dir.path().join("data.db"),
        &crate::base64_encode(&[3; 32]),
        prefs,
    )
    .unwrap();
    (dir, server)
}

fn offer(
    state: &ServerState,
    client: &crate::crypto::EcdhSession,
    peer: Value,
) -> ws_login::Request {
    use sha2::Digest;
    let digest = crate::utils::hex::bytes_to_hex(&sha2::Sha512::digest(b"secret"));
    let request = json!({"password":digest,"browserName":"Test","browserVersion":"1","osName":"Fixture","osVersion":"1","isMobile":false,"ecdhPublicKey":crate::base64_encode(&client.public_key_bytes),"peer":peer});
    let frame = crate::crypto::xchacha_encrypt_raw(
        &digest.as_bytes()[..32],
        &serde_json::to_vec(&request).unwrap(),
    )
    .unwrap();
    let _ = state;
    ws_login::Request::Issue {
        client_id: "browser".into(),
        client_ip: "192.0.2.1".into(),
        frame: crate::base64_encode(&frame),
    }
}

#[tokio::test]
async fn native_management_public_auth_and_revoke_share_one_database_without_host() {
    let (dir, server) = fixture();
    let state = server.runtime_state();
    let created = execute(&state, Request::Create { name: "API".into() }).unwrap();
    let id = created["client_id"].as_str().unwrap();
    let token = created["token"].as_str().unwrap();
    assert_eq!(crate::base64_decode(token).len(), 32);
    let mut headers = HeaderMap::new();
    headers.insert("c-id", id.parse().unwrap());
    headers.insert("authorization", format!("Bearer {token}").parse().unwrap());
    let response = main_graphql::call(
        State(state.clone()),
        headers.clone(),
        axum::body::Bytes::from(r#"{"query":"{noteCount(query:\"\")}"}"#),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        state
            .db
            .session_get(id)
            .unwrap()
            .unwrap()
            .last_active_at
            .is_some()
    );
    assert_eq!(
        execute(
            &state,
            Request::Rename {
                client_id: id.into(),
                name: "Renamed".into()
            }
        )
        .unwrap(),
        true
    );
    let reopened = crate::db::Db::open(&dir.path().join("data.db")).unwrap();
    assert_eq!(reopened.session_get(id).unwrap().unwrap().name, "Renamed");
    let mut wrong = headers.clone();
    wrong.insert("authorization", "Bearer wrong".parse().unwrap());
    assert_eq!(
        main_graphql::call(State(state.clone()), wrong, axum::body::Bytes::from("{}"))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    execute(
        &state,
        Request::Delete {
            client_id: id.into(),
        },
    )
    .unwrap();
    assert!(key(&state.db, id).unwrap().is_none());
    assert_eq!(
        main_graphql::call(State(state.clone()), headers, axum::body::Bytes::from("{}"))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert!(
        execute(&state, Request::List)
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
    let unauthorized = call(State(state), HeaderMap::new(), Json(Request::List)).await;
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn web_login_ecdh_encrypted_response_signature_and_token_auth_survive_restart() {
    let (dir, server) = fixture();
    let state = server.runtime_state();
    let client = crate::crypto::EcdhSession::generate();
    let issued = ws_login::execute(&state, offer(&state, &client, Value::Null))
        .await
        .unwrap();
    assert_eq!(issued["status"], "COMPLETED");
    let shared = client
        .compute_shared_key(&crate::base64_decode(
            issued["response"]["ecdhPublicKey"].as_str().unwrap(),
        ))
        .unwrap();
    assert_eq!(crate::base64_encode(&shared), issued["token"]);
    let row = state.db.session_get("browser").unwrap().unwrap();
    assert_eq!(row.r#type, "WEB");
    assert_eq!(
        crate::db::Db::open(&dir.path().join("data.db"))
            .unwrap()
            .session_get("browser")
            .unwrap()
            .unwrap()
            .token,
        row.token
    );
    use sha2::Digest;
    let digest = crate::utils::hex::bytes_to_hex(&sha2::Sha512::digest(b"secret"));
    let decrypted = crate::crypto::xchacha_decrypt_raw(
        &digest.as_bytes()[..32],
        &crate::base64_decode(issued["frame"].as_str().unwrap()),
    )
    .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&decrypted).unwrap(),
        issued["response"]
    );
    let response = &issued["response"];
    let material = format!(
        "phone|COMPLETED|{}|{}",
        response["ecdhPublicKey"].as_str().unwrap(),
        response["timestamp"].as_u64().unwrap()
    );
    let stored: Value =
        serde_json::from_str(&state.prefs.get_or("signature_key_pair", String::new())).unwrap();
    assert!(crate::crypto::ed25519_verify(
        stored["publicKey"].as_str().unwrap(),
        material.as_bytes(),
        response["signature"].as_str().unwrap()
    ));
    let mut headers = HeaderMap::new();
    headers.insert("c-id", "browser".parse().unwrap());
    let plaintext = format!(
        "{}|{}|{{\"query\":\"{{noteCount(query:\\\"\\\")}}\"}}",
        chrono::Utc::now().timestamp_millis(),
        uuid::Uuid::new_v4()
    );
    let response = main_graphql::call(
        State(state.clone()),
        headers.clone(),
        axum::body::Bytes::from(
            crate::crypto::xchacha_encrypt_raw(&shared, plaintext.as_bytes()).unwrap(),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let body = crate::crypto::xchacha_decrypt_raw(&shared, &body).unwrap();
    assert!(serde_json::from_slice::<Value>(&body).unwrap()["errors"].is_null());
    headers.insert(
        "authorization",
        format!("Bearer {}", row.token).parse().unwrap(),
    );
    assert_eq!(
        main_graphql::call(State(state), headers, axum::body::Bytes::from("{}"))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn confirmation_is_once_only_and_cancel_or_revoke_prevents_issuance() {
    let (_dir, server) = fixture();
    let state = server.runtime_state();
    state.prefs.set("auth_two_factor", true).unwrap();
    let client = crate::crypto::EcdhSession::generate();
    let pending = ws_login::execute(&state, offer(&state, &client, Value::Null))
        .await
        .unwrap();
    assert_eq!(pending["status"], "PENDING");
    assert!(state.db.session_get("browser").unwrap().is_none());
    let id = pending["requestId"].as_str().unwrap().to_owned();
    ws_login::execute(
        &state,
        ws_login::Request::Complete {
            request_id: id.clone(),
        },
    )
    .await
    .unwrap();
    assert!(
        ws_login::execute(&state, ws_login::Request::Complete { request_id: id })
            .await
            .is_err()
    );
    for cancelled in [true, false] {
        let pending = ws_login::execute(&state, offer(&state, &client, Value::Null))
            .await
            .unwrap();
        let id = pending["requestId"].as_str().unwrap().to_owned();
        if cancelled {
            ws_login::execute(
                &state,
                ws_login::Request::Cancel {
                    request_id: id.clone(),
                },
            )
            .await
            .unwrap();
        } else {
            execute(
                &state,
                Request::Delete {
                    client_id: "browser".into(),
                },
            )
            .unwrap();
        }
        assert!(
            ws_login::execute(&state, ws_login::Request::Complete { request_id: id })
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn malformed_encrypted_frames_consume_the_same_rust_login_limiter() {
    let (_dir, server) = fixture();
    let state = server.runtime_state();
    for _ in 0..5 {
        let error = ws_login::execute(
            &state,
            ws_login::Request::Issue {
                client_id: "browser".into(),
                client_ip: "192.0.2.1".into(),
                frame: "invalid".into(),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(error.to_string(), "invalid_password");
    }
    let client = crate::crypto::EcdhSession::generate();
    assert_eq!(
        ws_login::execute(&state, offer(&state, &client, Value::Null))
            .await
            .unwrap_err()
            .to_string(),
        "too_many_login_attempts"
    );
}

#[tokio::test]
async fn login_pairing_and_password_initialization_use_rust_state_only() {
    let (_dir, server) = fixture();
    let state = server.runtime_state();
    let client = crate::crypto::EcdhSession::generate();
    let (_, public) = crate::ed25519_generate();
    let peer = json!({"port":8443,"signaturePublicKey":crate::base64_encode(&public),"deviceName":"Desktop","deviceType":"COMPUTER","ips":["192.0.2.1"]});
    let pending = ws_login::execute(&state, offer(&state, &client, peer))
        .await
        .unwrap();
    assert_eq!(
        pending["status"], "PENDING",
        "pairing requires confirmation even with browser 2FA off"
    );
    let issued = ws_login::execute(
        &state,
        ws_login::Request::Complete {
            request_id: pending["requestId"].as_str().unwrap().into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(issued["response"]["chatPaired"], true);
    let paired = state.db.get_peer_by_id("browser").unwrap();
    use sha2::Digest;
    let expected = sha2::Sha256::digest(
        [
            b"plain-chat-pairing-v1".as_slice(),
            crate::base64_decode(issued["token"].as_str().unwrap()).as_slice(),
        ]
        .concat(),
    );
    assert_eq!(paired.key, crate::base64_encode(&expected));
    assert_eq!(paired.name, "Desktop");
    let token = key(&state.db, "browser").unwrap().unwrap();
    let mut headers = HeaderMap::new();
    headers.insert("c-id", "browser".parse().unwrap());
    let response = main_graphql::init(
        State(state.clone()),
        axum::extract::ConnectInfo("127.0.0.1:1".parse().unwrap()),
        headers.clone(),
        axum::body::Bytes::from(crate::crypto::xchacha_encrypt_raw(&token, b"init").unwrap()),
    )
    .await;
    let response: Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(response["password"], "");
    assert!(!response["signaturePublicKey"].as_str().unwrap().is_empty());
    let response = main_graphql::init(
        State(state.clone()),
        axum::extract::ConnectInfo("127.0.0.1:1".parse().unwrap()),
        headers,
        axum::body::Bytes::new(),
    )
    .await;
    let response: Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(response["password"].as_str().unwrap().len(), 6);
    assert_eq!(
        state.prefs.get_or("password", String::new()),
        response["password"]
    );
}
