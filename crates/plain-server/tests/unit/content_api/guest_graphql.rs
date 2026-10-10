use super::*;
use crate::{content_api::ContentServer, prefs::Prefs};
use std::sync::Arc;
#[tokio::test]
async fn guest_executes_encrypted_contract_and_rejects_replay_and_revocation() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("Shared");
    std::fs::create_dir(&folder).unwrap();
    let prefs = Arc::new(Prefs::load(&dir.path().join("prefs.json")).unwrap());
    prefs
        .set("master_secret", crate::base64_encode(&[11; 32]))
        .unwrap();
    prefs.set_user("service", true).unwrap();
    let server = ContentServer::start(
        &dir.path().join("db"),
        &crate::base64_encode(&[10; 32]),
        prefs,
    )
    .unwrap();
    let state = server.runtime_state();
    let service = Service::new(state.db.clone(), state.prefs.clone());
    let share = service
        .create(
            "Test".into(),
            vec![folder.to_str().unwrap().into()],
            crate::base64_encode(&[12; 32]),
            true,
            None,
        )
        .unwrap();
    let key =
        crate::utils::base64::base64_decode_checked(&service.token(&share.id).unwrap()).unwrap();
    let wrap = |nonce: &str, query: &str| {
        crate::xchacha_encrypt_raw(
            &key,
            format!("{}|{nonce}|{query}", chrono::Utc::now().timestamp_millis()).as_bytes(),
        )
        .unwrap()
    };
    let body = wrap(
        "a",
        r#"{"query":"query($path:String){ alias:sharedInfo(virtualPath:$path){name readOnly expiresAt entries{name virtualPath isDir size mimeType hasThumb}} }","variables":{"path":""}}"#,
    );
    let (status, encrypted) = execute(&state, &share.id, &body).await;
    assert_eq!(status, 200);
    let result: Value =
        serde_json::from_slice(&crate::xchacha_decrypt_raw(&key, &encrypted).unwrap()).unwrap();
    assert_eq!(result["data"]["alias"]["name"], "Test");
    assert_eq!(
        result["data"]["alias"]["entries"][0]["virtualPath"],
        "Shared/"
    );
    assert_eq!(result["data"]["alias"]["entries"][0]["size"], 0);
    assert_eq!(execute(&state, &share.id, &body).await.0, 400);
    assert_eq!(execute(&state, &share.id, &[0; 40]).await.0, 401);
    assert_eq!(execute(&state, "", &body).await.0, 401);
    let malformed = wrap("b", "invalid");
    assert_eq!(execute(&state, &share.id, &malformed).await.0, 400);
    let forbidden = wrap("c", r#"{"query":"{ shareRecords { id } }"}"#);
    let (status, encrypted) = execute(&state, &share.id, &forbidden).await;
    assert_eq!(status, 200);
    let result: Value =
        serde_json::from_slice(&crate::xchacha_decrypt_raw(&key, &encrypted).unwrap()).unwrap();
    assert!(result["errors"].is_array());
    state.db.share_delete(&share.id).unwrap();
    assert_eq!(execute(&state, &share.id, &body).await.0, 403);
    server.shutdown().await;
}
