use super::*;
use crate::content_api::public_schema::PublicSchema;
use crate::db::Db;
use crate::prefs::Prefs;
use serde_json::{Value, json};
use std::sync::Arc;

const GRANTED: &str = r#"["WRITE_EXTERNAL_STORAGE"]"#;

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

fn fixture(permissions: &str) -> (tempfile::TempDir, PublicSchema) {
    fixture_with(permissions, |method, _| match method {
        "systemPermissionFacts" => json!({ "granted": { "WRITE_EXTERNAL_STORAGE": true } }),
        other => panic!("unexpected host call {other}"),
    })
}

fn fixture_with(
    permissions: &str,
    handler: impl Fn(&str, Value) -> Value + Send + 'static,
) -> (tempfile::TempDir, PublicSchema) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&dir.path().join("prefs.json")).unwrap());
    prefs
        .set(
            "api_permissions",
            json!(serde_json::from_str::<Vec<String>>(permissions).unwrap()),
        )
        .unwrap();
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

fn dir_fact(name: &str, path: &str) -> Value {
    json!({
        "name": name, "path": path, "mediaId": "", "createdAt": Value::Null,
        "updatedAt": "2026-01-02T03:04:05Z", "size": 0, "isDir": true, "childCount": 0,
    })
}

#[tokio::test]
async fn file_writes_are_gated_on_storage_permission() {
    let (_dir, schema) = fixture("[]");
    let response = schema
        .execute(
            r#"mutation { deleteFiles(paths:["/a"]) { affectedCount }
                      createDir(path:"/a/b") { name }
                      renameFile(path:"/a", name:"c")
                      writeTextFile(path:"/a/b.txt", content:"x", overwrite:false) { name }
                      copyFile(src:"/a", dst:"/b", overwrite:false)
                      moveFile(src:"/a", dst:"/b", overwrite:false) }"#,
        )
        .await;
    assert_eq!(response.errors.len(), 6);
    for error in &response.errors {
        assert_eq!(error.message, "no_permission");
    }
}

/// The count is the delta, not the request size: a path that was already
/// gone did not delete anything.
#[tokio::test]
async fn delete_files_reports_what_was_actually_removed() {
    let (_dir, schema) = fixture_with(GRANTED, |method, params| match method {
        "systemPermissionFacts" => json!({ "granted": { "WRITE_EXTERNAL_STORAGE": true } }),
        "systemDeleteFiles" => {
            assert_eq!(params["paths"], json!(["/a", "/b", "/c"]));
            json!(2)
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"mutation { deleteFiles(paths:["/a","/b","/c"]) { affectedCount } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["deleteFiles"]["affectedCount"],
        2
    );
}

/// An empty path list is not a request; answering 0 without asking the
/// platform keeps a client bug from turning into a full-disk operation.
#[tokio::test]
async fn deleting_no_paths_is_a_no_op() {
    let (_dir, schema) = fixture_with(GRANTED, |method, _| match method {
        "systemPermissionFacts" => json!({ "granted": { "WRITE_EXTERNAL_STORAGE": true } }),
        other => panic!("the platform must not be asked: {other}"),
    });
    let response = schema
        .execute(r#"mutation { deleteFiles(paths:[]) { affectedCount } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["deleteFiles"]["affectedCount"],
        0
    );
}

#[tokio::test]
async fn a_created_directory_reads_back_as_a_file_row() {
    let (_dir, schema) = fixture_with(GRANTED, |method, params| match method {
        "systemPermissionFacts" => json!({ "granted": { "WRITE_EXTERNAL_STORAGE": true } }),
        "systemCreateDir" => {
            assert_eq!(params["path"], "/storage/DCIM");
            dir_fact("DCIM", "/storage/DCIM")
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"mutation { createDir(path:"/storage/DCIM") { name path isDir childCount mediaId updatedAt } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let row = response.data.into_json().unwrap();
    let row = &row["createDir"];
    assert_eq!(row["name"], "DCIM");
    assert_eq!(row["path"], "/storage/DCIM");
    assert_eq!(row["isDir"], true);
    assert_eq!(row["mediaId"], Value::Null);
    assert_eq!(row["updatedAt"], "2026-01-02T03:04:05.000Z");
}

/// A refused rename is an ordinary outcome, not a failure — an empty name,
/// a traversal segment or an occupied destination.
#[tokio::test]
async fn a_refused_rename_reports_false() {
    let (_dir, schema) = fixture_with(GRANTED, |method, _| match method {
        "systemPermissionFacts" => json!({ "granted": { "WRITE_EXTERNAL_STORAGE": true } }),
        "systemRenameFile" => json!(false),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"mutation { renameFile(path:"/a", name:"..") }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(response.data.into_json().unwrap()["renameFile"], false);
}

#[tokio::test]
async fn copy_and_move_carry_their_intent_to_the_platform() {
    let (_dir, schema) = fixture_with(GRANTED, |method, params| match method {
        "systemPermissionFacts" => json!({ "granted": { "WRITE_EXTERNAL_STORAGE": true } }),
        "systemTransferFile" => {
            assert_eq!(params["src"], "/a");
            assert_eq!(params["dst"], "/b");
            assert_eq!(params["overwrite"], true);
            assert!(params["type"] == "COPY" || params["type"] == "MOVE");
            json!(true)
        }
        other => panic!("unexpected host call {other}"),
    });
    for (name, expected) in [("copyFile", "COPY"), ("moveFile", "MOVE")] {
        let response = schema
            .execute(&format!(
                r#"mutation {{ {name}(src:"/a", dst:"/b", overwrite:true) }}"#
            ))
            .await;
        assert!(response.errors.is_empty(), "{name}: {:?}", response.errors);
        assert_eq!(response.data.into_json().unwrap()[name], true);
        let _ = expected;
    }
}

#[tokio::test]
async fn media_trash_and_restore_do_not_need_a_storage_grant() {
    let (_dir, schema) = fixture_with("[]", |method, params| match method {
        "systemPermissionFacts" => json!({ "granted": { "WRITE_EXTERNAL_STORAGE": true } }),
        "systemMediaAction" => {
            assert_eq!(params["action"], "trash");
            assert_eq!(params["dataType"], "IMAGE");
            json!(4)
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"mutation { trashMediaItems(type:IMAGE, query:"text:x") { affectedCount } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["trashMediaItems"]["affectedCount"],
        4
    );
}

/// Moving media is a storage write; the trash actions only move rows.
#[tokio::test]
async fn moving_media_is_gated() {
    let (_dir, schema) = fixture_with("[]", |method, _| match method {
        "systemPermissionFacts" => json!({ "granted": { "WRITE_EXTERNAL_STORAGE": true } }),
        other => panic!("the platform must not be asked: {other}"),
    });
    let response = schema
        .execute(r#"mutation { moveMediaItems(type:IMAGE, query:"text:x", destDir:"/d") { affectedCount } }"#)
        .await;
    assert_eq!(response.errors.len(), 1);
    assert_eq!(response.errors[0].message, "no_permission");
}

#[tokio::test]
async fn moving_media_forwards_the_destination() {
    let (_dir, schema) = fixture_with(GRANTED, |method, params| match method {
        "systemPermissionFacts" => json!({ "granted": { "WRITE_EXTERNAL_STORAGE": true } }),
        "systemMediaAction" => {
            assert_eq!(params["action"], "move");
            assert_eq!(params["dataType"], "DOC");
            assert_eq!(params["destDir"], "/storage/Docs");
            json!(2)
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"mutation { moveMediaItems(type:DOC, query:"text:x", destDir:"/storage/Docs") { affectedCount } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["moveMediaItems"]["affectedCount"],
        2
    );
}

/// A blank query matches every row of the kind. For a bulk delete that is
/// the difference between a mistake and data loss, so it is refused before
/// the platform is asked.
#[tokio::test]
async fn a_blank_media_query_is_refused_before_the_platform() {
    let (_dir, schema) = fixture_with(GRANTED, |method, _| match method {
        "systemPermissionFacts" => json!({ "granted": { "WRITE_EXTERNAL_STORAGE": true } }),
        other => panic!("the platform must not be asked: {other}"),
    });
    for name in [
        "deleteMediaItems",
        "trashMediaItems",
        "restoreMediaItems",
        "moveMediaItems",
    ] {
        let document = if name == "moveMediaItems" {
            format!(
                r#"mutation {{ {name}(type:IMAGE, query:"", destDir:"/d") {{ affectedCount }} }}"#
            )
        } else {
            format!(r#"mutation {{ {name}(type:IMAGE, query:"   ") {{ affectedCount }} }}"#)
        };
        let response = schema.execute(&document).await;
        assert_eq!(response.errors.len(), 1, "{name}");
        assert_eq!(
            response.errors[0].message, "explicit query required",
            "{name}"
        );
    }
}

#[tokio::test]
async fn chunk_state_is_listed_for_the_requested_upload() {
    let (_dir, schema) = fixture_with("[]", |method, params| match method {
        "systemUploadedChunkFacts" => {
            assert_eq!(params["fileId"], "abc");
            json!({ "chunks": ["0:1024", "1:2048"] })
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"query { uploadedChunks(fileId:"abc") }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["uploadedChunks"],
        json!(["0:1024", "1:2048"])
    );
}

/// An upload that was never started reads as NONE, not an error — the
/// client polls this before it has sent anything.
#[tokio::test]
async fn an_unstarted_merge_is_none_with_no_value() {
    let (_dir, schema) = fixture_with("[]", |method, _| match method {
        "systemMergeStatusFacts" => {
            json!({"status":"NONE","value":Value::Null,"mergedSize":Value::Null,"error":Value::Null})
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"query { mergeStatus(fileId:"x") { status value mergedSize error } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let task = response.data.into_json().unwrap()["mergeStatus"].clone();
    assert_eq!(task["status"], "NONE");
    assert_eq!(task["value"], Value::Null);
    assert_eq!(task["mergedSize"], Value::Null);
}

#[tokio::test]
async fn a_finished_merge_carries_its_value_and_size() {
    let (_dir, schema) = fixture_with("[]", |method, _| match method {
        "systemMergeStatusFacts" => {
            json!({"status":"DONE","value":"/storage/a.bin","mergedSize":4096,"error":Value::Null})
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"query { mergeStatus(fileId:"x") { status value mergedSize } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let task = response.data.into_json().unwrap()["mergeStatus"].clone();
    assert_eq!(task["status"], "DONE");
    assert_eq!(task["value"], "/storage/a.bin");
    assert_eq!(task["mergedSize"], 4096);
}

#[tokio::test]
async fn starting_a_merge_returns_started_and_the_arguments_reach_the_platform() {
    let (_dir, schema) = fixture_with("[]", |method, params| match method {
        "systemMergeChunks" => {
            assert_eq!(params["fileId"], "abc");
            assert_eq!(params["totalChunks"], 2);
            assert_eq!(params["path"], "/storage/a.bin");
            assert_eq!(params["replace"], true);
            assert_eq!(params["totalSize"], 8192);
            json!({"status":"STARTED"})
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"mutation { mergeChunks(fileId:"abc", totalChunks:2, path:"/storage/a.bin", replace:true, totalSize:8192)
                      { status } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["mergeChunks"]["status"],
        "STARTED"
    );
}

/// The app-file merge writes into the private store, so it never takes a
/// path or a replace flag — only a name hint.
#[tokio::test]
async fn the_app_file_merge_sends_only_a_name_hint() {
    let (_dir, schema) = fixture_with("[]", |method, params| match method {
        "systemMergeAppFileChunks" => {
            assert_eq!(params["fileId"], "abc");
            assert_eq!(params["fileName"], "photo.jpg");
            assert!(params.get("path").is_none());
            assert!(params.get("replace").is_none());
            json!({"status":"MERGING"})
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"mutation { mergeAppFileChunks(fileId:"abc", totalChunks:1, fileName:"photo.jpg", totalSize:10)
                      { status } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["mergeAppFileChunks"]["status"],
        "MERGING"
    );
}

#[tokio::test]
async fn a_failed_merge_keeps_its_error_and_omits_the_value() {
    let (_dir, schema) = fixture_with("[]", |method, _| match method {
        "systemMergeChunks" => json!({"status":"FAILED","error":"sum mismatch"}),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"mutation { mergeChunks(fileId:"abc", totalChunks:2, path:"/a", replace:false, totalSize:1)
                      { status value error } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let task = response.data.into_json().unwrap()["mergeChunks"].clone();
    assert_eq!(task["status"], "FAILED");
    assert_eq!(task["error"], "sum mismatch");
    assert_eq!(task["value"], Value::Null);
}

#[test]
fn merge_status_reads_back_the_names_the_contract_uses() {
    assert_eq!(MergeTaskStatus::parse("NONE"), MergeTaskStatus::None);
    assert_eq!(MergeTaskStatus::parse("STARTED"), MergeTaskStatus::Started);
    assert_eq!(MergeTaskStatus::parse("MERGING"), MergeTaskStatus::Merging);
    assert_eq!(MergeTaskStatus::parse("DONE"), MergeTaskStatus::Done);
    assert_eq!(MergeTaskStatus::parse("FAILED"), MergeTaskStatus::Failed);
    // An unrecognised state is "nothing happened yet" rather than a state
    // the client has never heard of.
    assert_eq!(MergeTaskStatus::parse("WAT"), MergeTaskStatus::None);
}

/// The predicate is worth nothing if the actions do not ask it, so this drives
/// the real mutation. Audio contributes no clause of its own, so `type:1` —
/// what the Apps view leaves behind when the user navigates to a media one —
/// used to resolve to the whole library.
#[tokio::test]
async fn a_media_action_refuses_a_query_that_selects_nothing_and_all_true_still_goes_through() {
    let (_dir, schema) = fixture_with(GRANTED, |method, _| match method {
        "systemPermissionFacts" => json!({ "granted": { "WRITE_EXTERNAL_STORAGE": true } }),
        "systemMediaAction" => json!(0),
        other => panic!("unexpected host call {other}"),
    });

    for mutation in [
        r#"mutation { deleteMediaItems(type: AUDIO, query: "type:1") { affectedCount } }"#,
        r#"mutation { trashMediaItems(type: AUDIO, query: "type:1") { affectedCount } }"#,
        r#"mutation { restoreMediaItems(type: AUDIO, query: "type:1") { affectedCount } }"#,
        r#"mutation { moveMediaItems(type: AUDIO, query: "type:1", destDir: "/sdcard/x") { affectedCount } }"#,
        r#"mutation { trashMediaItems(type: IMAGE, query: "show_hidden:false") { affectedCount } }"#,
    ] {
        let response = schema.execute(mutation).await;
        assert_eq!(response.errors.len(), 1, "{mutation} was not refused");
        assert!(
            response.errors[0].message.contains("selects no"),
            "{mutation} failed for the wrong reason: {}",
            response.errors[0].message
        );
    }

    // `all:true` builds no clause either, and says so — refusing it would
    // leave no way to target everything on purpose.
    let response = schema
        .execute(r#"mutation { trashMediaItems(type: AUDIO, query: "all:true") { affectedCount } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert!(format!("{:?}", response.data).contains("affectedCount"));
}
