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

fn sms_fact(id: &str) -> Value {
    json!({
        "id": id, "body": "hi", "address": "+1555", "date": "2026-10-05T00:00:00Z",
        "serviceCenter": "+1999", "read": true, "threadId": "t1", "type": 1,
        "subscriptionId": 2, "isMms": false,
        "attachments": [{"path":"/a.png","contentType":"image/png","name":"a.png"}],
    })
}

/// Everything the SMS roots need before a query reaches the platform.
fn sms_facts(schema: &PublicSchema, items: Value, count: i64) {
    stub(schema_host(schema), move |method, _| match method {
        "systemPermissionFacts" => json!({"granted":{"READ_SMS": true}}),
        "systemSmsRowsFacts" => json!({"items": items.clone(), "canonicalAddress": "+1555"}),
        "systemSmsCountFacts" => json!({"sms": count, "mms": 0}),
        other => panic!("unexpected host call {other}"),
    });
}

#[tokio::test]
async fn sms_reads_degrade_to_empty_without_the_platform_grant() {
    let (_dir, schema) = fixture(json!(["READ_SMS"]));
    stub(schema_host(&schema), |method, _| match method {
        "systemPermissionFacts" => json!({"granted":{"READ_SMS": false}}),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"query { sms(offset:0, limit:10, query:"") { id }
                 smsCount(query:"") smsConversationCount(query:"")
                 smsBoxCounts { total inbox sent drafts } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["sms"], json!([]));
    assert_eq!(data["smsCount"], 0);
    assert_eq!(data["smsConversationCount"], 0);
    assert_eq!(
        data["smsBoxCounts"],
        json!({"total":0,"inbox":0,"sent":0,"drafts":0})
    );
}

#[tokio::test]
async fn sms_maps_the_provider_rows_onto_the_contract_type() {
    let (_dir, schema) = fixture(json!(["READ_SMS"]));
    sms_facts(&schema, json!([sms_fact("m1")]), 3);
    let response = schema
        .execute(
            r#"query { sms(offset:0, limit:10, query:"") {
                 id body address sentAt serviceCenter read threadId type
                 subscriptionId isMms tags { id name count }
                 attachments { path contentType name } } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let message = &response.data.into_json().unwrap()["sms"][0];
    assert_eq!(message["id"], "m1");
    assert_eq!(message["type"], "INBOX");
    assert_eq!(message["sentAt"], "2026-10-05T00:00:00.000Z");
    assert_eq!(message["threadId"], "t1");
    assert_eq!(message["serviceCenter"], "+1999");
    assert_eq!(message["attachments"][0]["contentType"], "image/png");
    assert_eq!(message["tags"], json!([]));

    let response = schema.execute(r#"query { smsCount(query:"") }"#).await;
    assert_eq!(response.data.into_json().unwrap()["smsCount"], 3);
}

/// An unknown provider box must read back as UNKNOWN, not panic and not
/// silently become INBOX.
#[test]
fn unknown_sms_box_is_unknown() {
    assert_eq!(SmsType::from_android(0), SmsType::Unknown);
    assert_eq!(SmsType::from_android(99), SmsType::Unknown);
    assert_eq!(SmsType::from_android(6), SmsType::Queued);
}

#[tokio::test]
async fn bulk_sms_mutations_reject_a_blank_query_before_touching_the_platform() {
    let (_dir, schema) = fixture(json!(["READ_SMS"]));
    stub(schema_host(&schema), |method, _| {
        panic!("unexpected host call {method}")
    });
    for mutation in ["trashSms", "restoreSms", "deleteSms"] {
        let response = schema
            .execute(&format!(
                r#"mutation {{ {mutation}(query:"  ") {{ affectedCount }} }}"#
            ))
            .await;
        assert!(
            response.errors[0]
                .message
                .starts_with("query is required for bulk mutations"),
            "{mutation}: {:?}",
            response.errors
        );
    }
}

#[tokio::test]
async fn bulk_sms_mutations_pass_the_query_through_untouched() {
    let (_dir, schema) = fixture(json!(["READ_SMS"]));
    stub(schema_host(&schema), |method, params| match method {
        "systemTrashSms" => {
            assert_eq!(params["query"], "ids:1,2");
            json!(2)
        }
        "systemDeleteSms" => {
            assert_eq!(params["query"], "all:true");
            json!(7)
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"mutation { trashSms(query:"ids:1,2") { affectedCount } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["trashSms"]["affectedCount"],
        2
    );
    let response = schema
        .execute(r#"mutation { deleteSms(query:"all:true") { affectedCount } }"#)
        .await;
    assert_eq!(
        response.data.into_json().unwrap()["deleteSms"]["affectedCount"],
        7
    );
}

#[tokio::test]
async fn archiving_a_conversation_stores_the_last_message_date() {
    let (_dir, schema) = fixture(json!(["READ_SMS"]));
    stub(schema_host(&schema), |method, _| match method {
        "systemSmsConversationFacts" => json!({"items":[]}),
        "systemPermissionFacts" => json!({"granted":{"READ_SMS": true}}),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"mutation { archiveSmsConversation(id:"t1") }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["archiveSmsConversation"],
        true
    );
    // The conversation existed with no dated row, so today's date is stored.
    let db = schema.data::<Arc<Db>>().unwrap();
    let records = db.archived_conversation_list().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].conversation_id, "t1");
    let stored = chrono::DateTime::parse_from_rfc3339(&records[0].conversation_date).unwrap();
    assert!(
        chrono::Utc::now()
            .signed_duration_since(stored)
            .num_seconds()
            .abs()
            < 120,
        "stored {} is not 'now'",
        records[0].conversation_date
    );
}

#[tokio::test]
async fn send_sms_needs_the_send_permission_and_passes_the_sim_choice() {
    let (_dir, schema) = fixture(json!([]));
    stub(schema_host(&schema), |method, _| {
        panic!("unexpected host call {method}")
    });
    let response = schema
        .execute(
            r#"mutation { sendSms(number:"+1555", body:"hi", subscriptionId:2, requestId:"r1") }"#,
        )
        .await;
    assert_eq!(response.errors[0].message, "no_permission");

    let (_dir, schema) = fixture(json!(["SEND_SMS"]));
    stub(schema_host(&schema), |method, params| match method {
        "systemPermissionFacts" => json!({"granted":{"SEND_SMS": true}}),
        "systemSendSms" => {
            assert_eq!(params["number"], "+1555");
            assert_eq!(params["body"], "hi");
            assert_eq!(params["subscriptionId"], 2);
            assert_eq!(params["clientRequestId"], "r1");
            json!(true)
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"mutation { sendSms(number:"+1555", body:"hi", subscriptionId:2, requestId:"r1") }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(response.data.into_json().unwrap()["sendSms"], true);
}

/// -1 means "system default", which reaches the platform as no subscription.
#[tokio::test]
async fn send_sms_maps_the_default_sim_to_no_subscription() {
    let (_dir, schema) = fixture(json!(["SEND_SMS"]));
    stub(schema_host(&schema), |method, params| match method {
        "systemSendSms" => {
            assert_eq!(params["subscriptionId"], Value::Null);
            json!(true)
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"mutation { sendSms(number:"+1555", body:"hi", subscriptionId:-1, requestId:"r2") }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
}

#[tokio::test]
async fn send_mms_allocates_one_rust_pending_id_and_launches_only_a_native_intent() {
    let (_dir, schema) = fixture(json!(["SEND_SMS"]));
    stub(schema_host(&schema), |method, params| match method {
        "systemMmsLatest"=>json!(7),
        "systemMmsLaunch"=>{assert_eq!(params["number"],"+1555");assert_eq!(params["attachments"],json!([]));json!(100)},
        "systemMmsCandidates"=>json!([]),
        other=>panic!("Unexpected platform primitive: {other}"),
    });
    let response=schema.execute(r#"mutation {sendMms(number:"+1555",body:"hi",attachmentPaths:[],threadId:"t1")}"#).await;
    assert!(response.errors.is_empty(),"{:?}",response.errors);
    let id=response.data.into_json().unwrap()["sendMms"].as_str().unwrap().to_owned();
    assert!(id.starts_with("pending_mms_"));
    let runtime=schema.data::<Arc<super::super::mms_send::Runtime>>().unwrap();
    assert_eq!(runtime.snapshot()[0]["id"],id);
    assert_eq!(runtime.snapshot()[0]["threadId"],"t1");
    let duplicate=schema.execute(r#"mutation {sendMms(number:"+1555",body:"hi",attachmentPaths:[],threadId:"t1")}"#).await;
    assert_eq!(duplicate.errors.len(),1);
    assert!(duplicate.errors[0].message.contains("already pending"));
    runtime.cancel_all();
}
