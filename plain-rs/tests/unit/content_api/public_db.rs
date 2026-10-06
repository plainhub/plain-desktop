use super::*;
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
    (
        dir,
        crate::content_api::public_schema::build(host, prefs, db),
    )
}

#[tokio::test]
async fn the_browser_reads_the_platform_database_not_the_rust_one() {
    let (_dir, schema) = fixture(|method, _| match method {
        "systemDbFacts" => json!({
            "path": "/data/user/0/com.ismartcoding.plain/databases/plain.db",
            "tables": ["notes", "chats"],
        }),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema.execute(r#"query { dbPath dbTables }"#).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(
        data["dbPath"],
        "/data/user/0/com.ismartcoding.plain/databases/plain.db"
    );
    assert_eq!(data["dbTables"], json!(["notes", "chats"]));
}

#[tokio::test]
async fn a_row_page_passes_offset_and_limit_through() {
    let (_dir, schema) = fixture(|method, params| match method {
        "systemDbRows" => {
            assert_eq!(params["table"], "notes");
            assert_eq!(params["offset"], 20);
            assert_eq!(params["limit"], 10);
            json!({ "rows": ["{\"id\":\"n1\"}", "{\"id\":\"n2\"}"] })
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"query { dbTableRows(table:"notes", offset:20, limit:10) }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let rows = response.data.into_json().unwrap()["dbTableRows"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0], "{\"id\":\"n1\"}");
}

#[tokio::test]
async fn columns_keep_their_flags_and_map_onto_the_column_types() {
    let (_dir, schema) = fixture(|method, _| match method {
        "systemDbColumns" => json!({ "columns": [
            {"name":"id","dataType":"TEXT","notNull":true,"defaultValue":null,"primaryKey":true},
            {"name":"size","dataType":"INTEGER","notNull":false,"defaultValue":"0","primaryKey":false},
            {"name":"ratio","dataType":"REAL","notNull":false,"defaultValue":null,"primaryKey":false},
            {"name":"thumb","dataType":"BLOB","notNull":false,"defaultValue":null,"primaryKey":false},
            {"name":"total","dataType":"NUMERIC","notNull":false,"defaultValue":null,"primaryKey":false},
            {"name":"weird","dataType":"GEOMETRY","notNull":false,"defaultValue":null,"primaryKey":false},
        ]}),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"query { dbTableColumns(table:"t") { name dataType notNull defaultValue primaryKey } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let columns = response.data.into_json().unwrap()["dbTableColumns"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(columns[0]["dataType"], "TEXT");
    assert_eq!(columns[0]["primaryKey"], true);
    assert_eq!(columns[0]["defaultValue"], Value::Null);
    assert_eq!(columns[1]["defaultValue"], "0");
    assert_eq!(columns[1]["notNull"], false);
    assert_eq!(columns[2]["dataType"], "REAL");
    assert_eq!(columns[3]["dataType"], "BLOB");
    assert_eq!(columns[4]["dataType"], "NUMERIC");
    // An unrecognised declared type is still a column; UNKNOWN keeps the
    // row renderable instead of failing the whole page.
    assert_eq!(columns[5]["dataType"], "UNKNOWN");
}

/// The developer console hands the browser whatever the operator typed, so
/// a rejected row reports `false` rather than blowing up the query.
#[tokio::test]
async fn a_rejected_row_write_reports_false() {
    let (_dir, schema) = fixture(|method, params| match method {
        "systemCreateDbRow" => {
            assert_eq!(params["table"], "notes");
            assert_eq!(params["row"], "not json");
            json!(false)
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"mutation { createDbTableRow(table:"notes", row:"not json") }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["createDbTableRow"],
        false
    );
}

#[tokio::test]
async fn deleting_rows_forwards_the_ids() {
    let (_dir, schema) = fixture(|method, params| match method {
        "systemDeleteDbRows" => {
            assert_eq!(params["table"], "notes");
            assert_eq!(params["ids"], json!(["n1", "n2"]));
            json!(true)
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"mutation { deleteDbTableRows(table:"notes", ids:["n1","n2"]) }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["deleteDbTableRows"],
        true
    );
}

/// Deleting nothing is not a request worth making: a table with no primary
/// key cannot be resolved, so the answer is false rather than a silent
/// success.
#[tokio::test]
async fn deleting_an_empty_id_list_is_refused_before_the_platform() {
    let (_dir, schema) = fixture(|method, _| panic!("the platform must not be asked: {method}"));
    let response = schema
        .execute(r#"mutation { deleteDbTableRows(table:"notes", ids:[]) }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["deleteDbTableRows"],
        false
    );
}

#[tokio::test]
async fn the_table_info_names_the_id_column() {
    let (_dir, schema) = fixture(|method, _| match method {
        "systemDbInfo" => json!({ "idKey": "note_id" }),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"query { dbTableInfo(table:"notes") { idKey } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["dbTableInfo"]["idKey"],
        "note_id"
    );
}

#[tokio::test]
async fn a_row_count_reads_as_a_long() {
    let (_dir, schema) = fixture(|method, params| match method {
        "systemDbRowCount" => {
            assert_eq!(params["table"], "chats");
            json!(9007199254740993i64)
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"query { dbTableRowCount(table:"chats") }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["dbTableRowCount"],
        9007199254740993i64
    );
}

#[test]
fn column_types_are_case_insensitive_and_total() {
    assert_eq!(DbColumnType::parse("text"), DbColumnType::Text);
    assert_eq!(DbColumnType::parse("INTEGER"), DbColumnType::Integer);
    assert_eq!(DbColumnType::parse("Int"), DbColumnType::Integer);
    assert_eq!(DbColumnType::parse("anything"), DbColumnType::Unknown);
    assert_eq!(DbColumnType::default(), DbColumnType::Unknown);
}
