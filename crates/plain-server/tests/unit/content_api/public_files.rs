use super::*;
use crate::content_api::public_schema::PublicSchema;
use crate::db::Db;
use crate::prefs::Prefs;
use serde_json::{Value, json};
use std::{fs, sync::Arc};

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

/// Grants storage by default and stubs `fileTaskAuthorize` to succeed, so a
/// test only has to name the host calls it actually cares about.
fn fixture(permissions: &str) -> (tempfile::TempDir, PublicSchema) {
    fixture_with(permissions, |method, params| match method {
        "fileTaskAuthorize" => {
            assert!(params["paths"].is_array());
            json!(true)
        }
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
    let (events, _) = tokio::sync::broadcast::channel(16);
    let schema = crate::content_api::public_schema::build(
        host.clone(),
        events,
        prefs,
        db,
        dir.path().into(),
    );
    stub(host, handler);
    (dir, schema)
}

fn schema_host(schema: &PublicSchema) -> Arc<Host> {
    schema.data::<Arc<Host>>().unwrap().clone()
}

fn sample_tree(dir: &std::path::Path) -> std::path::PathBuf {
    let root = dir.join("storage");
    fs::create_dir_all(root.join("nested")).unwrap();
    fs::write(root.join("alpha.txt"), b"alpha").unwrap();
    fs::write(root.join("beta.png"), b"beta").unwrap();
    fs::write(root.join("nested/gamma.txt"), b"gamma").unwrap();
    root
}

#[tokio::test]
async fn files_are_gated_on_storage_permission() {
    let (_dir, schema) = fixture("[]");
    let response = schema
        .execute(
            r#"query { files(root:"/storage", offset:0, limit:10, query:"", sortBy:NAME_ASC) { name } }"#,
        )
        .await;
    assert_eq!(response.errors.len(), 1);
    assert_eq!(response.errors[0].message, "no_permission");
}

#[tokio::test]
async fn a_directory_page_is_read_through_the_shared_filesystem_walk() {
    let (dir, schema) = fixture(GRANTED);
    let root = sample_tree(dir.path());
    let response = schema
        .execute(&format!(
            r#"query {{ files(root:{:?}, offset:0, limit:10, query:"", sortBy:NAME_ASC)
                      {{ name path size isDir childCount mediaId }} }}"#,
            root.to_string_lossy()
        ))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    let items = data["files"].as_array().unwrap();
    let names: Vec<&str> = items
        .iter()
        .map(|item| item["name"].as_str().unwrap())
        .collect();
    // Directories first, then files by name — the same ordering the app's
    // own file browser sees.
    assert_eq!(names, vec!["nested", "alpha.txt", "beta.png"]);
    assert_eq!(items[0]["isDir"], true);
    assert_eq!(items[1]["isDir"], false);
    assert_eq!(items[1]["size"], 5);
    assert_eq!(items[0]["childCount"], 1);
    // The empty sentinel is a null id, never "".
    assert_eq!(items[1]["mediaId"], Value::Null);
}

#[tokio::test]
async fn a_file_count_ignores_paging_but_honours_the_query() {
    let (dir, schema) = fixture(GRANTED);
    let root = sample_tree(dir.path());
    let response = schema
        .execute(&format!(
            r#"query {{ all: fileCount(root:{:?}, query:"")
                      txt: fileCount(root:{:?}, query:".txt") }}"#,
            root.to_string_lossy(),
            root.to_string_lossy()
        ))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["all"], 3);
    // A bare token is the DSL's name filter, and it searches recursively —
    // so gamma.txt counts even though it is one level down.
    assert_eq!(data["txt"], 2);
}

#[tokio::test]
async fn the_page_and_the_count_describe_the_same_query() {
    let (dir, schema) = fixture(GRANTED);
    let root = sample_tree(dir.path());
    let response = schema
        .execute(&format!(
            r#"query {{ files(root:{:?}, offset:0, limit:2, query:"", sortBy:SIZE_DESC) {{ name }}
                      fileCount(root:{:?}, query:"") }}"#,
            root.to_string_lossy(),
            root.to_string_lossy()
        ))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["files"].as_array().unwrap().len(), 2);
    assert_eq!(data["fileCount"], 3);
}

#[tokio::test]
async fn file_info_carries_the_stat_and_the_platform_media_probe() {
    let (dir, schema) = fixture_with(GRANTED, |method, _| match method {
        "fileTaskAuthorize" => json!(true),
        "systemImageFileInfo" => {
            json!({"width": 4032, "height": 3024, "location": {"latitude": 1.5, "longitude": -2.5}})
        }
        other => panic!("unexpected host call {other}"),
    });
    let path = dir.path().join("pic.jpg");
    fs::write(&path, b"jpeg").unwrap();
    let response = schema
        .execute(&format!(
            r#"query {{ fileInfo(path:{:?}) {{ path size updatedAt
                      data {{ ... on ImageFileInfo {{ width height location {{ latitude longitude }} }} }} }} }}"#,
            path.to_string_lossy()
        ))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["fileInfo"]["size"], 4);
    assert_eq!(data["fileInfo"]["data"]["width"], 4032);
    assert_eq!(data["fileInfo"]["data"]["height"], 3024);
    assert_eq!(data["fileInfo"]["data"]["location"]["latitude"], 1.5);
    assert_eq!(data["fileInfo"]["data"]["location"]["longitude"], -2.5);
}

/// The probe is picked by the name, not by the bytes: a display name that
/// carries the extension selects the decoder even when the path does not.
#[tokio::test]
async fn the_media_probe_is_selected_by_the_supplied_name() {
    let (dir, schema) = fixture_with(GRANTED, |method, _| match method {
        "fileTaskAuthorize" => json!(true),
        "systemAudioFileInfo" => json!({"durationMs": 221000, "location": Value::Null}),
        other => panic!("unexpected host call {other}"),
    });
    let path = dir.path().join("blob");
    fs::write(&path, b"opus").unwrap();
    let response = schema
        .execute(&format!(
            r#"query {{ fileInfo(path:{:?}, fileName:"track.m4a")
                      {{ data {{ ... on AudioFileInfo {{ durationMs location {{ latitude longitude }} }} }} }} }}"#,
            path.to_string_lossy()
        ))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["fileInfo"]["data"]["durationMs"], 221000);
    assert_eq!(data["fileInfo"]["data"]["location"], Value::Null);
}

/// A plain-text name is not media: no probe runs at all, and `data` is null.
#[tokio::test]
async fn a_non_media_name_probes_nothing() {
    let (dir, schema) = fixture(GRANTED);
    let path = dir.path().join("notes.txt");
    fs::write(&path, b"notes").unwrap();
    let response = schema
        .execute(&format!(
            r#"query {{ fileInfo(path:{:?}) {{ data {{ ... on ImageFileInfo {{ width }} }} }} }}"#,
            path.to_string_lossy()
        ))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["fileInfo"]["data"],
        Value::Null
    );
}

/// A path that cannot be resolved is not an error: the panel renders zeros
/// so a stale link degrades instead of blowing up the whole page.
#[tokio::test]
async fn a_missing_path_reports_zeros_rather_than_failing() {
    let (_dir, schema) = fixture(GRANTED);
    let response = schema
        .execute(r#"query { fileInfo(path:"/nope/missing.jpg") { path size updatedAt data { ... on ImageFileInfo { width } } } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["fileInfo"]["size"], 0);
    assert_eq!(data["fileInfo"]["updatedAt"], "1970-01-01T00:00:00.000Z");
    assert_eq!(data["fileInfo"]["data"], Value::Null);
}

#[tokio::test]
async fn path_predicates_answer_for_files_and_directories() {
    let (dir, schema) = fixture(GRANTED);
    let root = sample_tree(dir.path());
    let file = root.join("alpha.txt");
    let response = schema
        .execute(&format!(
            r#"query {{ f: pathExists(path:{:?}) fk: pathKind(path:{:?})
                      d: pathExists(path:{:?}) dk: pathKind(path:{:?}) }}"#,
            file.to_string_lossy(),
            file.to_string_lossy(),
            root.to_string_lossy(),
            root.to_string_lossy()
        ))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["f"], true);
    assert_eq!(data["fk"], "FILE");
    assert_eq!(data["d"], true);
    assert_eq!(data["dk"], "DIR");
}

/// The total predicate: blank, "." and missing all read false / null and
/// none of them raise, so clients can filter on it unguarded.
#[tokio::test]
async fn unresolvable_paths_are_false_rather_than_errors() {
    let (_dir, schema) = fixture(GRANTED);
    let response = schema
        .execute(
            r#"query { blank: pathExists(path:"") dot: pathExists(path:".")
                      missing: pathExists(path:"/nope")
                      blankKind: pathKind(path:"") missingKind: pathKind(path:"/nope") }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["blank"], false);
    assert_eq!(data["dot"], false);
    assert_eq!(data["missing"], false);
    assert_eq!(data["blankKind"], Value::Null);
    assert_eq!(data["missingKind"], Value::Null);
}

/// A refused root is indistinguishable from a missing one on purpose —
/// `pathExists` must not become a probe for what the client may read.
#[tokio::test]
async fn a_denied_root_reads_as_absent() {
    let (_dir, schema) = fixture_with(GRANTED, |method, _| match method {
        "fileTaskAuthorize" => json!(false),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"query { exists: pathExists(path:"/data/user/0/secret") kind: pathKind(path:"/data/user/0/secret") }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["exists"], false);
    assert_eq!(data["kind"], Value::Null);
}

/// The decoder must not run for a path the authorization refused: the
/// probe reads the file's header, so running it on a denied path would
/// hand a client dimensions and GPS for bytes it may not read. The stub
/// panics on any media call, so this fails loudly if one is made.
#[tokio::test]
async fn a_denied_path_is_never_probed_for_media() {
    let (_dir, schema) = fixture_with(GRANTED, |method, _| match method {
        "fileTaskAuthorize" => json!(false),
        other => panic!("media probe ran for a denied path: {other}"),
    });
    let response = schema
        .execute(
            r#"query { fileInfo(path:"/data/user/0/secret.jpg")
                      { path size data { ... on ImageFileInfo { width } } } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["fileInfo"]["size"], 0);
    assert_eq!(data["fileInfo"]["data"], Value::Null);
}

/// These two carry no permission gate: enumerating volumes and reading the
/// bookmarks table is not a storage read of user content.
#[tokio::test]
async fn mounts_and_favorites_need_no_storage_grant() {
    let (_dir, schema) = fixture_with("[]", |method, _| match method {
        "systemMountFacts" => json!([
            {"id":"path:/storage","name":"Internal","path":"/storage","mountPoint":"/storage",
             "fsType":"ext4","totalBytes":100,"usedBytes":40,"freeBytes":60,"remote":false,
             "alias":"","driveType":"INTERNAL_STORAGE","diskId":""},
            {"id":"path:/storage/1234-5678","name":"","path":"/storage/1234-5678",
             "mountPoint":"/storage/1234-5678","fsType":"vfat","totalBytes":10,"usedBytes":4,
             "freeBytes":6,"remote":false,"alias":"USB","driveType":"USB_STORAGE","diskId":"1234"},
        ]),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"query { mounts { id name mountPoint fsType totalBytes usedBytes freeBytes
                                remote alias driveType diskId } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let mounts = response.data.into_json().unwrap();
    let mounts = mounts["mounts"].as_array().unwrap();
    assert_eq!(mounts[0]["driveType"], "INTERNAL_STORAGE");
    assert_eq!(mounts[0]["totalBytes"], 100);
    assert_eq!(mounts[1]["driveType"], "USB_STORAGE");
    assert_eq!(mounts[1]["alias"], "USB");
}

/// The bookmarks come straight out of Rust SQLite — the host is never
/// asked, because doing so would re-enter this root once `main_graphql`
/// serves the public schema.
#[tokio::test]
async fn favorite_folders_are_read_from_the_rust_store() {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&dir.path().join("prefs.json")).unwrap());
    let db = Arc::new(Db::open(&dir.path().join("data.db")).unwrap());
    let host = Arc::new(Host::default());
    stub(host.clone(), |method, _| {
        panic!("unexpected host call {method}")
    });
    crate::library::favorite_folders::add_full_path(
        &db,
        "/storage/emulated/0",
        "/storage/emulated/0/DCIM",
    )
    .unwrap();
    crate::library::favorite_folders::add_full_path(
        &db,
        "/storage/emulated/0",
        "/storage/emulated/0/Download",
    )
    .unwrap();
    crate::library::favorite_folders::set_alias(&db, "/storage/emulated/0", "DCIM", "Camera")
        .unwrap();
    let (events, _) = tokio::sync::broadcast::channel(16);
    let schema =
        crate::content_api::public_schema::build(host, events, prefs, db, dir.path().into());

    let response = schema
        .execute(r#"query { favoriteFolders { rootPath fullPath alias } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let items = response.data.into_json().unwrap();
    let items = items["favoriteFolders"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["fullPath"], "/storage/emulated/0/DCIM");
    assert_eq!(items[0]["alias"], "Camera");
    assert_eq!(items[1]["fullPath"], "/storage/emulated/0/Download");
    assert_eq!(items[1]["alias"], Value::Null);
}

#[tokio::test]
async fn recent_files_are_gated_and_projected_from_the_platform() {
    let (_dir, schema) = fixture_with(GRANTED, |method, _| match method {
        "systemRecentFileFacts" => json!([
            {"name":"a.txt","path":"/storage/a.txt","mediaId":"","createdAt":1700000000000i64,
             "updatedAt":1700000001000i64,"size":12,"isDir":false,"childCount":0},
            {"name":"b.mp4","path":"/storage/b.mp4","mediaId":"42","createdAt":Value::Null,
             "updatedAt":1700000002000i64,"size":4096,"isDir":false,"childCount":0},
        ]),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"query { recentFiles { name mediaId createdAt updatedAt size } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    let items = data["recentFiles"].as_array().unwrap();
    assert_eq!(items[0]["mediaId"], Value::Null);
    assert_eq!(items[0]["updatedAt"], "2023-11-14T22:13:21.000Z");
    assert_eq!(items[1]["mediaId"], "42");
    assert_eq!(items[1]["createdAt"], Value::Null);
    assert_eq!(items[1]["size"], 4096);
}

/// Every mutation returns the whole list, so a client re-renders from the
/// response instead of guessing what happened to rows it did not touch.
#[tokio::test]
async fn favorite_folder_mutations_return_the_whole_list() {
    let (_dir, schema) = fixture("[]");
    let response = schema
        .execute(
            r#"mutation { addFavoriteFolder(rootPath:"/storage/emulated/0",
                                fullPath:"/storage/emulated/0/DCIM") { fullPath alias } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["addFavoriteFolder"],
        json!([{"fullPath": "/storage/emulated/0/DCIM", "alias": null}])
    );

    let response = schema
        .execute(
            r#"mutation { setFavoriteFolderAlias(fullPath:"/storage/emulated/0/DCIM", alias:"Camera")
                          { fullPath alias } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let list = response.data.into_json().unwrap();
    let list = &list["setFavoriteFolderAlias"];
    assert_eq!(list[0]["alias"], "Camera");

    let response = schema
        .execute(r#"mutation { removeFavoriteFolder(fullPath:"/storage/emulated/0/DCIM") { fullPath } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["removeFavoriteFolder"],
        json!([])
    );
}

/// Removing a folder that is already gone is a no-op, not an error: the
/// client cannot tell the two apart and would only show a spurious failure.
#[tokio::test]
async fn removing_an_unknown_favorite_folder_is_a_no_op() {
    let (_dir, schema) = fixture("[]");
    let response = schema
        .execute(r#"mutation { removeFavoriteFolder(fullPath:"/nope") { fullPath } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["removeFavoriteFolder"],
        json!([])
    );
    let response = schema
        .execute(r#"mutation { setFavoriteFolderAlias(fullPath:"/nope", alias:"x") { fullPath } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["setFavoriteFolderAlias"],
        json!([])
    );
}

/// `TAKEN_AT_DESC` has no filesystem equivalent — it is an EXIF-time sort
/// the directory walk cannot do, so it must not be mistaken for a name
/// sort that would silently return a different page.
#[test]
fn sort_by_maps_onto_what_the_directory_walk_can_do() {
    assert!(matches!(
        FileSortBy::DateDesc.into(),
        crate::filesystem::SortBy::DateDesc
    ));
    assert!(matches!(
        FileSortBy::SizeAsc.into(),
        crate::filesystem::SortBy::SizeAsc
    ));
    assert!(matches!(
        FileSortBy::NameDesc.into(),
        crate::filesystem::SortBy::NameDesc
    ));
    assert!(matches!(
        FileSortBy::TakenAtDesc.into(),
        crate::filesystem::SortBy::NameAsc
    ));
}
