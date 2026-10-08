use super::*;
use crate::content_api::public_schema::PublicSchema;
use serde_json::{Value, json};

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
    let db = Arc::new(Db::open(&dir.path().join("data.db")).unwrap());
    let host = Arc::new(Host::default());
    let directory = dir.path().to_path_buf();
    let (events, _) = tokio::sync::broadcast::channel(16);
    (
        dir,
        crate::content_api::public_schema::build(host, events, prefs, db, directory),
    )
}

fn schema_host(schema: &PublicSchema) -> Arc<Host> {
    schema.data::<Arc<Host>>().unwrap().clone()
}

fn call_fact(id: &str, kind: i64) -> Value {
    json!({
        "id": id, "number": "+8618012345678", "name": "Ada", "photoUri": "",
        "startedAt": "2026-10-05T00:00:00Z", "durationSec": 42, "type": kind,
        "accountId": "acc-1",
        "geo": {"country":"US","numberType":"MOBILE","carrier":"Test","description":"mobile"},
    })
}

/// The direction code the platform does not recognise reads back as UNKNOWN.
#[test]
fn unknown_call_direction_is_unknown() {
    assert_eq!(CallType::from_android(0), CallType::Unknown);
    assert_eq!(CallType::from_android(99), CallType::Unknown);
    assert_eq!(CallType::from_android(7), CallType::AnsweredExternally);
    assert_eq!(CallType::from_android(3), CallType::Missed);
}

#[tokio::test]
async fn calls_map_the_provider_rows_onto_the_contract_type() {
    let (_dir, schema) = fixture(json!(["READ_CALL_LOG"]));
    stub(schema_host(&schema), |method, params| match method {
        "systemCallFacts" => {
            assert_eq!(params["query"], "text:ada");
            assert_eq!(params["offset"], 0);
            assert_eq!(params["limit"], 20);
            json!([call_fact("c1", 2)])
        }
        "systemPhoneLocaleFacts"=>json!({"region":"CN","locale":"zh_CN","available":true}),
        "systemPhoneMetadata"=>json!({"carrier":"Test","description":"mobile"}),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"query { calls(offset:0, limit:20, query:"text:ada") {
                 id number name photoId startedAt durationSec type accountId
                 tags { id name count }
                 geo { country numberType carrier description } } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let call = &response.data.into_json().unwrap()["calls"][0];
    assert_eq!(call["id"], "c1");
    assert_eq!(call["type"], "OUTGOING");
    assert_eq!(call["durationSec"], 42);
    assert_eq!(call["startedAt"], "2026-10-05T00:00:00.000Z");
    assert_eq!(call["accountId"], "acc-1");
    assert_eq!(call["geo"]["numberType"], "MOBILE");
    assert_eq!(call["tags"], json!([]));
}

#[tokio::test]
async fn call_count_degrades_to_zero_without_the_platform_grant() {
    let (_dir, schema) = fixture(json!(["READ_CALL_LOG"]));
    stub(schema_host(&schema), |method, _| match method {
        "systemPermissionFacts" => json!({"granted":{"WRITE_CALL_LOG": false}}),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema.execute(r#"query { callCount(query:"") }"#).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(response.data.into_json().unwrap()["callCount"], 0);
}

#[tokio::test]
async fn call_reads_refuse_to_answer_without_the_web_permission() {
    let (_dir, schema) = fixture(json!([]));
    stub(schema_host(&schema), |method, _| {
        panic!("unexpected host call {method}")
    });
    let response = schema
        .execute(r#"query { calls(offset:0, limit:1, query:"") { id } }"#)
        .await;
    assert_eq!(response.errors[0].message, "no_permission");
}

#[tokio::test]
async fn placing_a_call_needs_the_call_phone_permission() {
    let (_dir, schema) = fixture(json!(["READ_CALL_LOG"]));
    stub(schema_host(&schema), |method, _| {
        panic!("unexpected host call {method}")
    });
    let response = schema
        .execute(r#"mutation { call(number:"+1555", showDialer:false) }"#)
        .await;
    assert_eq!(response.errors[0].message, "no_permission");

    let (_dir, schema) = fixture(json!(["CALL_PHONE"]));
    stub(schema_host(&schema), |method, params| match method {
        "systemMakeCall" => {
            assert_eq!(params["number"], "+1555");
            assert_eq!(params["showDialer"], true);
            json!(true)
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"mutation { call(number:"+1555", showDialer:true) }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(response.data.into_json().unwrap()["call"], true);
}

#[tokio::test]
async fn delete_calls_counts_only_what_the_platform_deleted() {
    let (_dir, schema) = fixture(json!(["WRITE_CALL_LOG"]));
    stub(schema_host(&schema), |method, params| match method {
        "systemCallIds" => {
            assert_eq!(params["query"], "all:true");
            json!(["a", "b"])
        }
        "systemDeleteRecords" => {
            assert_eq!(params["provider"], "CALL");
            assert_eq!(params["ids"], json!(["a", "b"]));
            json!(["b"])
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"mutation { deleteCalls(query:"all:true") { affectedCount } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["deleteCalls"]["affectedCount"],
        1
    );
}

#[tokio::test]
async fn delete_calls_rejects_a_blank_query_before_touching_the_platform() {
    let (_dir, schema) = fixture(json!(["WRITE_CALL_LOG"]));
    stub(schema_host(&schema), |method, _| {
        panic!("unexpected host call {method}")
    });
    let response = schema
        .execute(r#"mutation { deleteCalls(query:" ") { affectedCount } }"#)
        .await;
    assert!(
        response.errors[0]
            .message
            .starts_with("query is required for bulk mutations"),
        "{:?}",
        response.errors
    );
}

/// Call contributes no clause of its own — it has no trash scope and no
/// duration floor — so every field the web client carries into it from a media
/// view builds nothing, and an empty clause list is `1=1` on the way to the
/// platform. That is the whole call log.
#[tokio::test]
async fn delete_calls_refuses_a_query_of_fields_it_does_not_act_on() {
    let (_dir, schema) = fixture(json!(["WRITE_CALL_LOG"]));
    stub(schema_host(&schema), |method, _| {
        panic!("a refused delete must not reach the platform: {method}")
    });
    for query in ["trash:false", "show_hidden:false"] {
        let response = schema
            .execute(&format!(
                r#"mutation {{ deleteCalls(query:"{query}") {{ affectedCount }} }}"#
            ))
            .await;
        assert_eq!(response.errors.len(), 1, "{query} was not refused");
        assert!(
            response.errors[0].message.contains("selects no"),
            "{query} failed for the wrong reason: {}",
            response.errors[0].message
        );
    }
}
