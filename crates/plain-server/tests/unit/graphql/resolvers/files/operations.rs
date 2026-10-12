use super::*;
use crate::graphql::schemas::public::PublicSchema;
use plain_rs::db::Db;
use plain_rs::prefs::Prefs;
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
    prefs.set("client_id", "file-test").unwrap();
    let db = Arc::new(Db::open(&dir.path().join("data.db")).unwrap());
    let host = Arc::new(Host::default());
    stub(host.clone(), handler);
    let directory = dir.path().to_path_buf();
    let (events, _) = tokio::sync::broadcast::channel(16);
    (
        dir,
        crate::graphql::schemas::public::build(host, events, prefs, db, directory),
    )
}

fn file_platform(method: &str, _params: Value) -> Value {
    match method {
        "systemPermissionFacts" => json!({"granted":{"WRITE_EXTERNAL_STORAGE":true}}),
        "fileTaskAuthorize" | "fileTaskScan" => json!(true),
        "fileTaskMediaSnapshot" => json!([]),
        other => panic!("unexpected platform operation {other}"),
    }
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
    let (dir, schema) = fixture_with(GRANTED, file_platform);
    let paths = ["a", "b", "missing"].map(|name| dir.path().join(name));
    for path in &paths[..2] {
        std::fs::write(path, "value").unwrap();
    }
    let response = schema
        .execute(
            async_graphql::Request::new(
                "mutation($paths:[String!]!) {deleteFiles(paths:$paths){affectedCount}}",
            )
            .variables(async_graphql::Variables::from_json(json!({"paths":paths}))),
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["deleteFiles"]["affectedCount"],
        2
    );
    assert!(paths.iter().all(|path| !path.exists()));
}

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
    let (dir, schema) = fixture_with(GRANTED, file_platform);
    let path = dir.path().join("DCIM");
    let response = schema.execute(async_graphql::Request::new("mutation($path:String!){createDir(path:$path){name path isDir childCount mediaId updatedAt}}")
        .variables(async_graphql::Variables::from_json(json!({"path":path})))).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let value = response.data.into_json().unwrap();
    let row = &value["createDir"];
    assert_eq!(row["name"], "DCIM");
    assert_eq!(row["path"], json!(path));
    assert_eq!(row["isDir"], true);
    assert_eq!(row["childCount"], 0);
    assert!(row["mediaId"].is_null());
    assert!(chrono::DateTime::parse_from_rfc3339(row["updatedAt"].as_str().unwrap()).is_ok());
    assert!(path.is_dir());
}

#[tokio::test]
async fn a_refused_rename_reports_false() {
    let (_dir, schema) = fixture_with(GRANTED, |method, _| match method {
        "systemPermissionFacts" => json!({ "granted": { "WRITE_EXTERNAL_STORAGE": true } }),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"mutation { renameFile(path:"/a", name:"..") }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(response.data.into_json().unwrap()["renameFile"], false);
}

#[tokio::test]
async fn copy_and_move_use_the_shared_file_tasks() {
    let (dir, schema) = fixture_with(GRANTED, file_platform);
    let source = dir.path().join("a");
    let destination = dir.path().join("b");
    for name in ["copyFile", "moveFile"] {
        std::fs::write(&source, "new").unwrap();
        std::fs::write(&destination, "old").unwrap();
        let response = schema.execute(async_graphql::Request::new(format!("mutation($src:String!,$dst:String!){{{name}(src:$src,dst:$dst,overwrite:true)}}"))
            .variables(async_graphql::Variables::from_json(json!({"src":source,"dst":destination})))).await;
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        assert_eq!(response.data.into_json().unwrap()[name], true);
        assert_eq!(std::fs::read_to_string(&destination).unwrap(), "new");
        assert_eq!(source.exists(), name == "copyFile");
        let tasks = schema
            .data::<Arc<crate::services::files::file_tasks::FileTasks>>()
            .unwrap()
            .list("file-test".into())
            .await
            .unwrap();
        assert!(
            tasks
                .iter()
                .any(|task| task.title == if name == "copyFile" { "COPY" } else { "MOVE" })
        );
    }
}

#[tokio::test]
async fn media_trash_and_restore_do_not_need_a_storage_grant() {
    let (_dir, schema) = fixture_with("[]", |method, params| match method {
        "systemPermissionFacts" => json!({ "granted": { "WRITE_EXTERNAL_STORAGE": true } }),
        "systemMediaIdsFacts" => {
            assert_eq!(params["dataType"], "IMAGE");
            json!((1..=4).map(|i| i.to_string()).collect::<Vec<_>>())
        }
        "mediaAction" => {
            assert_eq!(params["action"], "TRASH");
            json!({"successful":params["ids"].as_array().unwrap().iter().map(|id| json!({"id":id,"path":format!("/media/{}",id.as_str().unwrap()),"destinationPath":"/item"})).collect::<Vec<_>>(),"failedIds":[]})
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
        "systemMediaIdsFacts" => {
            assert_eq!(params["dataType"], "DOC");
            json!((1..=2).map(|i| i.to_string()).collect::<Vec<_>>())
        }
        "mediaAction" => {
            assert_eq!(params["action"], "MOVE");
            assert_eq!(params["destDir"], "/storage/Docs");
            json!({"successful":params["ids"].as_array().unwrap().iter().map(|id| json!({"id":id,"path":format!("/media/{}",id.as_str().unwrap()),"destinationPath":"/storage/Docs/item"})).collect::<Vec<_>>(),"failedIds":[]})
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

fn upload_fixture() -> (tempfile::TempDir, PublicSchema, std::path::PathBuf) {
    let (dir, schema) = fixture_with("[]", |method, _| match method {
        "scanFilesFacts" => json!(true),
        other => panic!("Unexpected platform primitive: {other}"),
    });
    let base = dir.path().join("uploads");
    (dir, schema, base)
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
    let (temp, schema, base) = upload_fixture();
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
    let (temp, schema, base) = upload_fixture();
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
    let (_temp, schema, base) = upload_fixture();
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
        "systemMediaIdsFacts" => json!([]),
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

#[tokio::test]
async fn media_batches_commit_successes_and_notify_only_confirmed_ids() {
    let batches = Arc::new(std::sync::Mutex::new(Vec::new()));
    let calls = batches.clone();
    let (_dir, schema) = fixture_with(GRANTED, move |method, params| match method {
        "systemAppFacts" => json!({"capabilities":["MEDIA_TRASH"]}),
        "systemMediaIdsFacts" => {
            assert_eq!(params["fromTrash"], true);
            json!(
                (0..501)
                    .map(|i| i.to_string())
                    .chain(["0".into()])
                    .collect::<Vec<_>>()
            )
        }
        "mediaAction" => {
            assert_eq!(params["fromTrash"], true);
            let ids = params["ids"].as_array().unwrap();
            calls.lock().unwrap().push(ids.len());
            let successful = ids.iter().filter(|id| **id != json!("1")).map(|id| json!({"id":id,"path":format!("/media/{}", id.as_str().unwrap()),"destinationPath":""})).collect::<Vec<_>>();
            let failed = ids
                .iter()
                .filter(|id| **id == json!("1"))
                .cloned()
                .collect::<Vec<_>>();
            json!({"successful":successful,"failedIds":failed})
        }
        other => panic!("unexpected platform operation {other}"),
    });
    let db = schema.data::<Arc<Db>>().unwrap();
    let tag =
        plain_rs::tags::create_tag(db, plain_rs::enums::DataType::Video.kind(), "keep failed")
            .unwrap();
    plain_rs::tags::add_relations(
        db,
        &[(tag.id.clone(), "0".into()), (tag.id.clone(), "1".into())],
    )
    .unwrap();
    let mut events = schema
        .data::<tokio::sync::broadcast::Sender<plain_rs::events::WsEvent>>()
        .unwrap()
        .subscribe();
    let response = schema
        .execute(r#"mutation {deleteMediaItems(type:VIDEO,query:"all:true"){affectedCount}}"#)
        .await;
    assert_eq!(response.errors.len(), 1);
    assert!(
        response.errors[0]
            .message
            .contains("1 media actions failed (500 completed)")
    );
    assert_eq!(*batches.lock().unwrap(), vec![500, 1]);
    assert_eq!(
        plain_rs::tags::keys_for_tag(db, &tag.id).unwrap(),
        vec!["1"]
    );
    let mut committed = Vec::new();
    let mut refreshes = 0;
    while let Ok(event) = events.try_recv() {
        if event.event_type == "CONTENT_CHANGED" {
            assert!(!event.is_internal());
            refreshes += 1;
        }
        if event.event_type == plain_rs::events::MEDIA_ACTION_FINISHED {
            assert!(event.is_internal());
            let payload: Value = serde_json::from_str(&event.payload).unwrap();
            assert_eq!(payload["type"], 2);
            assert_eq!(payload["action"], "DELETE");
            committed.extend(
                payload["ids"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|id| id.as_str().unwrap().to_owned()),
            );
        }
    }
    assert_eq!(refreshes, 2);
    assert_eq!(committed.len(), 500);
    assert!(!committed.contains(&"1".to_string()));
}
