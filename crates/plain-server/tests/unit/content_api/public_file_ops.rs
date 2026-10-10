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

fn upload_fixture() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    PublicSchema,
    std::path::PathBuf,
) {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().join("chunks");
    let path = base.clone();
    let (dir, schema) = fixture_with("[]", move |method, _| match method {
        "uploadTmpDirFacts" => json!({"path":path}),
        other => panic!("Unexpected platform primitive: {other}"),
    });
    (temp, dir, schema, base)
}
async fn merge_status(schema: &PublicSchema, id: &str) -> Value {
    let response = schema
        .execute(format!(
            r#"{{mergeStatus(fileId:"{id}"){{status value mergedSize error}}}}"#
        ))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    response.data.into_json().unwrap()["mergeStatus"].clone()
}
async fn wait_merge(schema: &PublicSchema, id: &str) -> Value {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let status = merge_status(schema, id).await;
        if matches!(status["status"].as_str(), Some("DONE" | "FAILED")) {
            return status;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "Merge did not complete: {status}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}
#[tokio::test]
async fn graphql_uploads_list_merge_and_recover_the_same_rust_job() {
    let (temp, _dir, schema, base) = upload_fixture();
    let chunks = base.join("abc");
    tokio::fs::create_dir_all(&chunks).await.unwrap();
    tokio::fs::write(chunks.join("chunk_1"), b"def")
        .await
        .unwrap();
    tokio::fs::write(chunks.join("chunk_0"), b"abc")
        .await
        .unwrap();
    let response = schema.execute(r#"{uploadedChunks(fileId:"abc")}"#).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["uploadedChunks"],
        json!(["0:3", "1:3"])
    );
    assert_eq!(merge_status(&schema, "abc").await["status"], "NONE");
    let target = temp.path().join("result.bin");
    let response=schema.execute(format!(r#"mutation {{mergeChunks(fileId:"abc",totalChunks:2,path:"{}",replace:true,totalSize:6){{status}}}}"#,target.display())).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["mergeChunks"]["status"],
        "STARTED"
    );
    let status = wait_merge(&schema, "abc").await;
    assert_eq!(status["status"], "DONE");
    assert_eq!(status["value"], "result.bin");
    assert_eq!(status["mergedSize"], 6);
    assert_eq!(status["error"], Value::Null);
    assert_eq!(tokio::fs::read(&target).await.unwrap(), b"abcdef");
    let response=schema.execute(format!(r#"mutation {{mergeChunks(fileId:"abc",totalChunks:2,path:"{}",replace:true,totalSize:6){{status}}}}"#,target.display())).await;
    assert!(response.errors.is_empty());
    assert_eq!(
        response.data.into_json().unwrap()["mergeChunks"]["status"],
        "DONE"
    );
}
#[tokio::test]
async fn graphql_failed_merge_keeps_error_and_no_value() {
    let (temp, _dir, schema, base) = upload_fixture();
    let chunks = base.join("abc");
    tokio::fs::create_dir_all(&chunks).await.unwrap();
    tokio::fs::write(chunks.join("chunk_0"), b"abc")
        .await
        .unwrap();
    let response=schema.execute(format!(r#"mutation {{mergeChunks(fileId:"abc",totalChunks:2,path:"{}",replace:false,totalSize:6){{status}}}}"#,temp.path().join("result").display())).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let status = wait_merge(&schema, "abc").await;
    assert_eq!(status["status"], "FAILED");
    assert!(
        status["error"]
            .as_str()
            .unwrap()
            .contains("Missing chunk 1")
    );
    assert_eq!(status["value"], Value::Null);
    assert!(chunks.exists());
}
#[tokio::test]
async fn graphql_app_file_merge_imports_into_the_rust_store() {
    let (_temp, _dir, schema, base) = upload_fixture();
    let chunks = base.join("abc");
    tokio::fs::create_dir_all(&chunks).await.unwrap();
    tokio::fs::write(chunks.join("chunk_0"), b"image")
        .await
        .unwrap();
    let response=schema.execute(r#"mutation {mergeAppFileChunks(fileId:"abc",totalChunks:1,fileName:"photo.jpg",totalSize:5){status}}"#).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let status = wait_merge(&schema, "abc").await;
    assert_eq!(status["status"], "DONE");
    assert!(!status["value"].as_str().unwrap().is_empty());
    assert_eq!(status["mergedSize"], 5);
    assert!(!chunks.exists());
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
        .execute(
            r#"mutation { trashMediaItems(type: AUDIO, query: "all:true") { affectedCount } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert!(format!("{:?}", response.data).contains("affectedCount"));
}
