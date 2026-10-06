use super::*;
use crate::content_api::public_schema::PublicSchema;
use crate::db::Db;
use serde_json::json;

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

fn fixture(permissions: Value) -> (tempfile::TempDir, PublicSchema) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&dir.path().join("prefs.json")).unwrap());
    prefs.set("api_permissions", permissions).unwrap();
    let db = Arc::new(Db::open(std::path::Path::new(":memory:")).unwrap());
    let host = Arc::new(Host::default());
    (
        dir,
        crate::content_api::public_schema::build(host, prefs, db),
    )
}

fn fact(id: &str, app: &str, time: &str) -> Value {
    json!({
        "id":id,"onlyOnce":true,"isClearable":true,"appId":app,"appName":app,
        "time":time,"silent":false,"title":"title","body":"body",
        "actions":["Reply","Open"],"replyActions":["Reply"],
    })
}

fn schema_host(schema: &PublicSchema) -> Arc<Host> {
    schema.data::<Arc<Host>>().unwrap().clone()
}

#[tokio::test]
async fn notifications_map_host_facts_into_the_contract_type() {
    let (_dir, schema) = fixture(json!(["NOTIFICATION_LISTENER"]));
    stub(schema_host(&schema), |method, _| match method {
        "systemNotificationFacts" => json!([
            fact("0|1|com.a", "com.a", "2026-10-05T01:00:00Z"),
            fact("0|2|com.b", "com.b", "2026-10-05T02:00:00Z"),
        ]),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"query { notifications(offset:0, limit:10, query:"") {
                   id onlyOnce isClearable appId appName postedAt silent title body
                   actions replyActions } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let items = response.data.into_json().unwrap()["notifications"].clone();
    assert_eq!(items[0]["id"], "0|2|com.b", "newest first");
    assert_eq!(items[0]["appId"], "com.b");
    assert_eq!(items[0]["postedAt"], "2026-10-05T02:00:00.000Z");
    assert_eq!(items[0]["actions"], json!(["Reply", "Open"]));
    assert_eq!(items[0]["replyActions"], json!(["Reply"]));
}

#[tokio::test]
async fn notification_roots_error_out_without_the_api_permission() {
    let (_dir, schema) = fixture(json!([]));
    let response = schema
        .execute(r#"query { notifications(offset:0, limit:1, query:"") { id } }"#)
        .await;
    assert_eq!(response.errors[0].message, "no_permission");
}

#[tokio::test]
async fn delete_notifications_counts_only_what_the_platform_confirmed() {
    let (_dir, schema) = fixture(json!(["NOTIFICATION_LISTENER"]));
    stub(schema_host(&schema), |method, params| match method {
        "systemCancelNotifications" => {
            assert_eq!(params["ids"], json!(["a", "b"]));
            json!(["a"])
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"mutation { deleteNotifications(ids:["a","b"]) { affectedCount } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["deleteNotifications"]["affectedCount"],
        1
    );
}

#[tokio::test]
async fn a_receipt_naming_an_unrequested_notification_is_rejected() {
    let (_dir, schema) = fixture(json!(["NOTIFICATION_LISTENER"]));
    stub(schema_host(&schema), |method, _| match method {
        "systemCancelNotifications" => json!(["never-requested"]),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"mutation { deleteNotifications(ids:["a"]) { affectedCount } }"#)
        .await;
    assert_eq!(
        response.errors[0].message,
        "invalid notification cancellation receipt"
    );
}

#[tokio::test]
async fn reply_notification_reports_a_vanished_action_as_action_not_found() {
    let (_dir, schema) = fixture(json!(["NOTIFICATION_LISTENER"]));
    stub(schema_host(&schema), |method, params| match method {
        "systemReplyNotification" => {
            assert_eq!(params["id"], "a");
            assert_eq!(params["actionIndex"], 0);
            assert_eq!(params["text"], "hi");
            json!(false)
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"mutation { replyNotification(id:"a", actionIndex:0, text:"hi") }"#)
        .await;
    assert_eq!(response.errors[0].message, "action_not_found");
}

/// A negative index never reaches the platform: plain-app's `getOrNull`
/// would return null there too, but it must not reach the host either.
#[tokio::test]
async fn reply_notification_rejects_a_negative_index_without_calling_the_host() {
    let (_dir, schema) = fixture(json!(["NOTIFICATION_LISTENER"]));
    stub(schema_host(&schema), |method, _| {
        panic!("unexpected host call {method}")
    });
    let response = schema
        .execute(r#"mutation { replyNotification(id:"a", actionIndex:-1, text:"hi") }"#)
        .await;
    assert_eq!(response.errors[0].message, "action_not_found");
}
