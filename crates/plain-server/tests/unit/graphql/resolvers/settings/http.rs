use crate::assembly::server::{ContentServer, ServerState};
use crate::http::{routes::graphql, sessions::host};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
};
use plain_rs::prefs::Prefs;
use serde_json::{Value, json};
use std::sync::Arc;

fn device() -> (tempfile::TempDir, ContentServer, HeaderMap) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&dir.path().join("system_prefs.json")).unwrap());
    plain_rs::prefs::system::bootstrap(&prefs).unwrap();
    let server = ContentServer::start(
        &dir.path().join("data.db"),
        &plain_rs::base64_encode(&[3; 32]),
        prefs,
    )
    .unwrap();
    let session = host::execute(
        &server.runtime_state(),
        host::Request::Create {
            name: "Controller".into(),
        },
    )
    .unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        "c-id",
        session["client_id"].as_str().unwrap().parse().unwrap(),
    );
    headers.insert(
        "authorization",
        format!("Bearer {}", session["token"].as_str().unwrap())
            .parse()
            .unwrap(),
    );
    (dir, server, headers)
}
async fn call(state: ServerState, headers: &HeaderMap, document: &str) -> (StatusCode, Value) {
    let response = graphql::call(
        State(state),
        headers.clone(),
        axum::body::Bytes::from(json!({"query":document}).to_string()),
    )
    .await;
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}
#[tokio::test]
async fn remote_settings_authentication_device_isolation_and_disable_reply() {
    let (_a, first, headers) = device();
    let (_b, second, other) = device();
    let (status,result)=call(first.runtime_state(),&headers,"mutation {updateUserSettings(input:{editorFontSize:20,feedAutoRefreshIntervalSec:3600}) {result {affectedCount} userSettings {editorFontSize feedAutoRefreshIntervalSec}}}").await;
    assert_eq!(status, StatusCode::OK);
    assert!(result.get("errors").is_none(), "{result}");
    assert_eq!(
        result["data"]["updateUserSettings"]["userSettings"]["editorFontSize"],
        20
    );
    let (_, result) = call(
        second.runtime_state(),
        &other,
        "{userSettings {editorFontSize}} ",
    )
    .await;
    assert_eq!(result["data"]["userSettings"]["editorFontSize"], 14);
    let (_, result) = call(first.runtime_state(), &headers, "{systemPrefs}").await;
    assert!(
        result["data"]["systemPrefs"]
            .get("signature_key_pair")
            .is_none()
    );
    assert!(result["data"]["systemPrefs"].get("master_secret").is_none());
    assert_eq!(
        call(
            second.runtime_state(),
            &headers,
            "{userSettings {editorFontSize}}"
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (_, result) = call(
        first.runtime_state(),
        &headers,
        "mutation {updateUserSettings(input:{desktopAccess:false}) {userSettings {desktopAccess}}}",
    )
    .await;
    assert_eq!(
        result["data"]["updateUserSettings"]["userSettings"]["desktopAccess"],
        false
    );
    assert_eq!(
        call(
            first.runtime_state(),
            &headers,
            "{userSettings {editorFontSize}}"
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(
            second.runtime_state(),
            &other,
            "{userSettings {editorFontSize}}"
        )
        .await
        .0,
        StatusCode::OK
    );
    let local = reqwest::Client::new().post(format!("http://127.0.0.1:{}/graphql",first.port))
        .bearer_auth(plain_rs::base64_encode(&[3;32])).header("content-type","application/json")
        .body(json!({"query":"mutation($key: String!, $value: JSON!) {setSystemPref(key:$key,value:$value) {affectedCount}}", "variables":{"key":"sms_send_test","value":"result"}}).to_string())
        .send().await.unwrap().text().await.unwrap();
    let local: Value = serde_json::from_str(&local).unwrap();
    assert!(local.get("errors").is_none(), "{local}");
    let snapshot = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{}/graphql", first.port))
        .bearer_auth(plain_rs::base64_encode(&[3; 32]))
        .header("content-type", "application/json")
        .body(json!({"query":"{systemPrefs userSettings {desktopAccess}}"}).to_string())
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let snapshot: Value = serde_json::from_str(&snapshot).unwrap();
    assert_eq!(snapshot["data"]["systemPrefs"]["sms_send_test"], "result");
    assert_eq!(snapshot["data"]["userSettings"]["desktopAccess"], false);
    first.shutdown().await;
    second.shutdown().await;
}
