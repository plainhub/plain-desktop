use super::*;
use crate::content_api::host::Host;
use crate::content_api::public_schema::PublicSchema;
use crate::db::Db;
use crate::prefs::Prefs;
use serde_json::{Value, json};
use std::sync::Arc;

fn stub(host: Arc<Host>, handler: impl Fn(&str, Value) -> Value + Send + 'static) {
    let (generation, mut requests) = host.connect();
    let host = host.clone();
    tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let Some(id) = request["id"].as_u64() else {
                continue;
            };
            let result = handler(
                &request["method"].as_str().unwrap_or_default(),
                request["params"].clone(),
            );
            let _ = host.reply(generation, json!({ "id": id, "result": result }));
        }
    });
}

fn fixture(
    handler: impl Fn(&str, Value) -> Value + Send + 'static,
) -> (tempfile::TempDir, PublicSchema) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&dir.path().join("prefs.json")).unwrap());
    let db = Arc::new(Db::open(&dir.path().join("data.db")).unwrap());
    let host = Arc::new(Host::default());
    stub(host.clone(), handler);
    let directory = dir.path().to_path_buf();
    let (events, _) = tokio::sync::broadcast::channel(16);
    (
        dir,
        crate::content_api::public_schema::build(host, events, prefs, db, directory),
    )
}

#[tokio::test]
async fn reads_come_from_the_rust_store() {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&dir.path().join("prefs.json")).unwrap());
    let db = Arc::new(Db::open(&dir.path().join("data.db")).unwrap());
    let host = Arc::new(Host::default());
    stub(host.clone(), |method, _| {
        panic!("a read must not reach the platform: {method}")
    });
    let (events, _) = tokio::sync::broadcast::channel(16);
    let schema = crate::content_api::public_schema::build(
        host,
        events,
        prefs.clone(),
        db,
        dir.path().into(),
    );

    prefs.set("device_name", "Pixel").unwrap();
    prefs.set_user("recent_search", json!(["a", "b"])).unwrap();

    let response = schema.execute(r#"query { systemPrefs userPrefs }"#).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["systemPrefs"]["device_name"], "Pixel");
    assert_eq!(data["userPrefs"]["recent_search"], json!(["a", "b"]));
    // The two snapshots are disjoint stores: a user pref is not a system one.
    assert!(data["systemPrefs"].get("recent_search").is_none());
    assert!(data["userPrefs"].get("device_name").is_none());
}

/// A pref write has to travel through the platform so its in-memory copy
/// and the flows the UI collects see the change. The stub below fails the
/// test if the write is faked locally, which is the whole point.
#[tokio::test]
async fn a_pref_write_goes_through_the_platform() {
    let (_dir, schema) = fixture(|method, params| match method {
        "systemSetUserPref" => {
            assert_eq!(params["key"], "recent_search");
            assert_eq!(params["value"], json!(["x"]));
            json!(true)
        }
        "systemRemoveUserPref" => {
            assert_eq!(params["key"], "recent_search");
            json!(true)
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"mutation { setUserPref(key:"recent_search", value:["x"]) }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(response.data.into_json().unwrap()["setUserPref"], true);

    let response = schema
        .execute(r#"mutation { removeUserPref(key:"recent_search") }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(response.data.into_json().unwrap()["removeUserPref"], true);
}

/// The key ends up in a file name and a JSON object; anything outside this
/// set is refused before it can reach either.
#[tokio::test]
async fn pref_keys_outside_the_safe_set_are_refused() {
    let (_dir, schema) = fixture(|method, _| panic!("the platform must not be asked: {method}"));
    for key in ["", "has space", "a/b", "a$b", "emoji🎉"] {
        let response = schema
            .execute(&format!(
                r#"mutation {{ setUserPref(key:{key:?}, value:"1") }}"#
            ))
            .await;
        assert_eq!(response.errors.len(), 1, "{key}");
        assert_eq!(response.errors[0].message, "invalid_pref_key", "{key}");
    }
    let response = schema
        .execute(&format!(
            r#"mutation {{ removeUserPref(key:"{}") }}"#,
            "a".repeat(129)
        ))
        .await;
    assert_eq!(response.errors.len(), 1);
    assert_eq!(response.errors[0].message, "invalid_pref_key");
}

/// The cap is on bytes, not characters - a multi-byte value would slip past
/// a character count and blow the store limit instead.
#[tokio::test]
async fn an_oversized_pref_value_is_refused_by_its_byte_length() {
    let (_dir, schema) = fixture(|method, _| panic!("the platform must not be asked: {method}"));
    // 33_000 four-byte characters is ~132 KB of JSON but only 33k characters.
    let payload: String = std::iter::repeat_n(char::from_u32(0x1F600).unwrap(), 33_000).collect();
    let value = serde_json::to_string(&payload).unwrap();
    let document = format!("mutation {{ setUserPref(key: \"big\", value: {value}) }}");
    let response = schema.execute(&document).await;
    assert_eq!(response.errors.len(), 1);
    assert_eq!(response.errors[0].message, "invalid_pref_value");
}

#[tokio::test]
async fn a_value_just_under_the_cap_is_accepted() {
    let (_dir, schema) = fixture(|method, _| match method {
        "systemSetUserPref" => json!(true),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"mutation { setUserPref(key:"ok", value:"0123456789") }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
}

#[test]
fn the_safe_key_set_is_the_one_the_platform_allows() {
    for key in ["a", "A", "0", "a_b", "a-b", "a.b", "screen_mirror_quality"] {
        assert!(validate_key(key).is_ok(), "{key}");
    }
    for key in ["", "a b", "a/b", "a$b", "a:b", "ä"] {
        assert_eq!(
            validate_key(key).unwrap_err().message,
            "invalid_pref_key",
            "{key}"
        );
    }
}
