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
