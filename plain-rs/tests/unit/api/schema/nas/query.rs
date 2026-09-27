//! Unit tests for `src/gql/query.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

/// Arc<Prefs> backed by a fresh prefs.json under `dir` (test fixture).
fn prefs(dir: &impl AsRef<std::path::Path>) -> std::sync::Arc<crate::prefs::Prefs> {
    std::sync::Arc::new(crate::prefs::Prefs::load(&dir.as_ref().join("prefs.json")).unwrap())
}



/// The paginated list ops take the shared DSL `query: String!` and the
/// server extracts only its `text:` field (plain-app contract).
#[test]
fn text_of_extracts_dsl_text_field() {
    assert_eq!(text_of("text:road"), "road");
    assert_eq!(text_of("parent:/a text:sea trash:true"), "sea");
    // A bare word is the DSL's implicit text field.
    assert_eq!(text_of("hello"), "hello");
    // No text field → empty (no filtering).
    assert_eq!(text_of("trash:true"), "");
    assert_eq!(text_of(""), "");
}

/// `app` serves the phone-contract enums: a NAS device type, the GITHUB
/// channel, and the storage permission the web media pages gate on (a NAS
/// always has access to its own storage; phone-only permissions stay
/// unreported).
#[tokio::test]
async fn app_serves_phone_contract_enums() {
    let dir = tempfile::tempdir().unwrap();
    let db = std::sync::Arc::new(crate::db::Db::open(dir.path()).unwrap());
    let config = std::sync::Arc::new(crate::config::Config::parse(
        "[server]\nhttp_port = 8080\nhttps_port = 8443\n",
    ));
    let schema = crate::gql::build_schema(
        db,
        prefs(&dir),
        config,
        dir.path().to_path_buf(),
        crate::test_support::chat_state(dir.path()),
    );

    let resp = schema
        .execute(async_graphql::Request::new(
            "{ app { deviceType buildChannel permissions } }",
        ))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    assert_eq!(
        resp.data.into_json().unwrap(),
        serde_json::json!({ "app": {
            "deviceType": "NAS",
            "buildChannel": "GITHUB",
            "permissions": ["WRITE_EXTERNAL_STORAGE"],
        } })
    );
}

/// The web home Files card gates its ScanPanel (pause/resume/stop/rebuild
/// index) on the `MEDIA_SCAN` capability, so `app.capabilities` must always
/// declare it alongside the always-on `MEDIA_TRASH`. (DOC_PREVIEW,
/// LAN_SHARE and DISK_MANAGER stay conditional on host probes — their
/// mapping is locked by `declared_capabilities_maps_host_probes_to_capabilities`,
/// not asserted against this machine.)
#[tokio::test]
async fn app_declares_media_scan_capability() {
    let dir = tempfile::tempdir().unwrap();
    let db = std::sync::Arc::new(crate::db::Db::open(dir.path()).unwrap());
    let config = std::sync::Arc::new(crate::config::Config::parse(
        "[server]\nhttp_port = 8080\nhttps_port = 8443\n",
    ));
    let schema = crate::gql::build_schema(
        db,
        prefs(&dir),
        config,
        dir.path().to_path_buf(),
        crate::test_support::chat_state(dir.path()),
    );

    let resp = schema
        .execute(async_graphql::Request::new("{ app { capabilities } }"))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    let capabilities: Vec<String> =
        serde_json::from_value(resp.data.into_json().unwrap()["app"]["capabilities"].clone())
            .unwrap();
    for required in ["MEDIA_SCAN", "MEDIA_TRASH"] {
        assert!(
            capabilities.iter().any(|c| c == required),
            "app.capabilities must declare {required}: {capabilities:?}"
        );
    }
}

/// The web client gates capability entries on `app.capabilities` alone —
/// never on `deviceType`. `declared_capabilities` maps each host probe to
/// its flag:
/// LAN_SHARE requires a loaded samba unit, DISK_MANAGER requires `lsblk`,
/// DOC_PREVIEW requires LibreOffice; MEDIA_TRASH/MEDIA_SCAN are always on.
#[test]
fn declared_capabilities_maps_host_probes_to_capabilities() {
    let all = declared_capabilities(true, true, true);
    for required in [
        Capability::MEDIA_TRASH,
        Capability::MEDIA_SCAN,
        Capability::LAN_SHARE,
        Capability::DISK_MANAGER,
        Capability::DOC_PREVIEW,
    ] {
        assert!(all.contains(&required), "missing {required:?} in {all:?}");
    }

    let bare = declared_capabilities(false, false, false);
    assert_eq!(bare, vec![Capability::MEDIA_TRASH, Capability::MEDIA_SCAN]);
    for absent in [
        Capability::LAN_SHARE,
        Capability::DISK_MANAGER,
        Capability::DOC_PREVIEW,
    ] {
        assert!(!bare.contains(&absent), "unexpected {absent:?} in {bare:?}");
    }

    // Each probe is independent: one capability on must not pull others in.
    assert_eq!(
        declared_capabilities(true, false, false),
        vec![
            Capability::MEDIA_TRASH,
            Capability::MEDIA_SCAN,
            Capability::LAN_SHARE
        ]
    );
    assert_eq!(
        declared_capabilities(false, true, false),
        vec![
            Capability::MEDIA_TRASH,
            Capability::MEDIA_SCAN,
            Capability::DISK_MANAGER
        ]
    );
}

/// Round-7 contract: `fileInfo` is path-addressed only — the `id` argument
/// is gone and `FileInfo` no longer carries `tags` (file tags lazy-load via
/// `tagRelations` on the client).
#[tokio::test]
async fn file_info_is_path_only_without_tags() {
    let base = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let dir = tempfile::Builder::new()
        .prefix("plainnas-test-")
        .tempdir_in(base)
        .unwrap();
    let db = std::sync::Arc::new(crate::db::Db::open(dir.path()).unwrap());
    let config = std::sync::Arc::new(crate::config::Config::parse(
        "[server]\nhttp_port = 8080\nhttps_port = 8443\n",
    ));
    let schema = crate::gql::build_schema(
        db,
        prefs(&dir),
        config,
        dir.path().to_path_buf(),
        crate::test_support::chat_state(dir.path()),
    );

    let song = dir.path().join("song.mp3");
    std::fs::write(&song, b"id3").unwrap();
    let lit = song
        .to_str()
        .unwrap()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");

    // id is no longer an accepted argument.
    let resp = schema
        .execute(async_graphql::Request::new(format!(
            r#"query {{ fileInfo(id: "K1", path: "{lit}") {{ path size }} }}"#
        )))
        .await;
    assert!(
        resp.errors.iter().any(|e| e.to_string().contains("id")),
        "expected unknown-argument error for `id`, got: {:?}",
        resp.errors
    );

    // Path-only query works; the `tags` selection is unknown.
    let resp = schema
        .execute(async_graphql::Request::new(format!(
            r#"query {{ fileInfo(path: "{lit}") {{ path size }} }}"#
        )))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    let data = resp.data.into_json().unwrap()["fileInfo"].clone();
    assert_eq!(data["path"], song.to_str().unwrap());
    let resp = schema
        .execute(async_graphql::Request::new(format!(
            r#"query {{ fileInfo(path: "{lit}") {{ tags {{ id }} }} }}"#
        )))
        .await;
    assert!(
        resp.errors.iter().any(|e| e.to_string().contains("tags")),
        "expected unknown-field error for `tags`, got: {:?}",
        resp.errors
    );
}

// ---------------------------------------------------------------------------
// Developer pages (plain-app contract): appLogs / dataStore / db / deviceInfo
// ---------------------------------------------------------------------------

/// Schema with its own temp store + data dir, in the `$HOME` scratch area
/// (the media exclusions refuse to index `/tmp` and the app data dir).
fn developer_schema() -> (
    crate::gql::AppSchema,
    std::path::PathBuf,
    std::sync::Arc<crate::db::Db>,
    std::sync::Arc<crate::prefs::Prefs>,
    tempfile::TempDir,
) {
    let base = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let dir = tempfile::Builder::new()
        .prefix("plainnas-dev-")
        .tempdir_in(base)
        .unwrap();
    let data_dir = dir.path().to_path_buf();
    let db = std::sync::Arc::new(crate::db::Db::open(dir.path()).unwrap());
    let prefs = prefs(&data_dir);
    let config = std::sync::Arc::new(crate::config::Config::parse(
        "[server]\nhttp_port = 8080\nhttps_port = 8443\n",
    ));
    (
        crate::gql::build_schema(
            db.clone(),
            prefs.clone(),
            config,
            data_dir.clone(),
            crate::test_support::chat_state(&data_dir),
        ),
        data_dir,
        db,
        prefs,
        dir,
    )
}

fn json_of(resp: &async_graphql::Response) -> serde_json::Value {
    resp.data.clone().into_json().unwrap()
}

/// The database/datastore pages' whole query surface against a seeded
/// store: table listing, per-table info/count/rows (with paging), and the
/// prefs.json DataStore listing.
#[tokio::test]
async fn developer_db_and_datastore_surface() {
    let (schema, data_dir, _db, prefs, _dir) = developer_schema();

    // Row data lives in the two SQLite stores; preferences in prefs.json.
    // build_schema opened <data_dir>/library.db — a second handle seeds it.
    let library = plain_rs::library::db::LibraryDb::open(&data_dir.join("library.db")).unwrap();
    library.with_conn(|conn| {
        conn.execute_batch(
            "INSERT INTO tags(id, type, name) VALUES ('devk1', 1, 'value-one');
             INSERT INTO tags(id, type, name) VALUES ('devk2', 2, 'value-two');",
        )
        .unwrap();
    });
    prefs.set("device_name", "box").unwrap();
    prefs
        .set("samba_settings", serde_json::json!({"enabled": false}))
        .unwrap();

    // Tables: both SQLite stores, bare names (no `chat.`/`library.`
    // prefix), sorted. The fjall KV namespaces (event/media/session) are
    // not tables.
    let resp = schema
        .execute(async_graphql::Request::new("{ dbTables }"))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    let tables: Vec<String> = serde_json::from_value(json_of(&resp)["dbTables"].clone()).unwrap();
    let mut sorted = tables.clone();
    sorted.sort();
    assert_eq!(tables, sorted);
    for expected in ["chats", "bookmarks", "tags", "audio_playlists"] {
        assert!(tables.contains(&expected.to_string()), "tables {tables:?}");
    }
    for gone in ["event", "media", "session", "chat.chats", "library.tags"] {
        assert!(!tables.contains(&gone.to_string()), "{gone} in {tables:?}");
    }

    // dbPath names both SQLite files; dataStorePath = prefs.json.
    let resp = schema
        .execute(async_graphql::Request::new("{ dbPath dataStorePath }"))
        .await;
    let json = json_of(&resp);
    let db_path = json["dbPath"].as_str().unwrap();
    let ds_path = json["dataStorePath"].as_str().unwrap();
    assert!(
        db_path.ends_with("chat.db") || db_path.contains("chat.db"),
        "{db_path}"
    );
    assert!(db_path.contains("library.db"), "{db_path}");
    assert_eq!(
        std::path::Path::new(ds_path),
        &crate::prefs::default_path(&data_dir),
        "dataStorePath is prefs.json under the data dir"
    );
    assert_ne!(db_path, ds_path);

    // Table info: rows identify by the declared primary key column.
    let resp = schema
        .execute(async_graphql::Request::new(
            r#"{ dbTableInfo(table: "tags") { idKey } }"#,
        ))
        .await;
    assert_eq!(
        json_of(&resp),
        serde_json::json!({ "dbTableInfo": { "idKey": "id" } })
    );
    let resp = schema
        .execute(async_graphql::Request::new(
            r#"{ dbTableInfo(table: "media") { idKey } }"#,
        ))
        .await;
    assert!(!resp.errors.is_empty(), "unknown table must error");

    // Count + rows on `tags` (2 seeded rows).
    let resp = schema
        .execute(async_graphql::Request::new(
            r#"{ dbTableRowCount(table: "tags") }"#,
        ))
        .await;
    assert_eq!(json_of(&resp)["dbTableRowCount"], serde_json::json!(2));

    let resp = schema
        .execute(async_graphql::Request::new(
            r#"{ dbTableRows(table: "tags", offset: 0, limit: 50) }"#,
        ))
        .await;
    let rows: Vec<String> = serde_json::from_value(json_of(&resp)["dbTableRows"].clone()).unwrap();
    assert_eq!(rows.len(), 2);
    let parsed: Vec<(String, i64)> = rows
        .iter()
        .map(|r| {
            let v: serde_json::Value = serde_json::from_str(r).unwrap();
            (
                v["id"].as_str().unwrap().to_string(),
                v["type"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        parsed,
        vec![("devk1".to_string(), 1), ("devk2".to_string(), 2),]
    );

    // Columns come from PRAGMA table_info, typed.
    let resp = schema
        .execute(async_graphql::Request::new(
            r#"{ dbTableColumns(table: "tags") { name dataType notNull defaultValue primaryKey } }"#,
        ))
        .await;
    assert_eq!(
        json_of(&resp)["dbTableColumns"],
        serde_json::json!([
            {"name": "id", "dataType": "TEXT", "notNull": false, "defaultValue": null, "primaryKey": true},
            {"name": "type", "dataType": "INTEGER", "notNull": true, "defaultValue": "0", "primaryKey": false},
            {"name": "name", "dataType": "TEXT", "notNull": true, "defaultValue": "''", "primaryKey": false},
        ])
    );

    // Paging windows into row order.
    let resp = schema
        .execute(async_graphql::Request::new(
            r#"{ dbTableRows(table: "tags", offset: 1, limit: 1) }"#,
        ))
        .await;
    let rows: Vec<String> = serde_json::from_value(json_of(&resp)["dbTableRows"].clone()).unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].contains("devk2"));

    // DataStore listing: the preferences map, key sorted, values rendered
    // as JSON (plain-desktop renders serde_json::Value::to_string()).
    let resp = schema
        .execute(async_graphql::Request::new(
            "{ dataStoreEntries { key value } }",
        ))
        .await;
    let entries = json_of(&resp)["dataStoreEntries"].clone();
    let got: Vec<(String, String)> = entries
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["key"].as_str().unwrap().to_string(),
                e["value"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![
            ("device_name".to_string(), "\"box\"".to_string()),
            (
                "samba_settings".to_string(),
                "{\"enabled\":false}".to_string()
            ),
        ],
        "fjall rows must not surface in the DataStore"
    );
}

/// The logs page: appLogPath points at `<data_dir>/logs/latest.log` and
/// appLogs streams newest-first with offset windows.
#[tokio::test]
async fn developer_logs_surface_newest_first() {
    let (schema, data_dir, _db, _prefs, _dir) = developer_schema();

    let log_file = crate::log::default_log_file(&data_dir);
    std::fs::create_dir_all(log_file.parent().unwrap()).unwrap();
    std::fs::write(&log_file, b"old-line\nmid-line\nnew-line\n").unwrap();

    let resp = schema
        .execute(async_graphql::Request::new("{ appLogPath }"))
        .await;
    assert_eq!(
        json_of(&resp)["appLogPath"].as_str().unwrap(),
        log_file.to_string_lossy()
    );

    let resp = schema
        .execute(async_graphql::Request::new(
            r#"{ appLogs(offset: 0, limit: 10, query: "") }"#,
        ))
        .await;
    assert_eq!(
        json_of(&resp)["appLogs"],
        serde_json::json!(["new-line", "mid-line", "old-line"])
    );

    let resp = schema
        .execute(async_graphql::Request::new(
            r#"{ appLogs(offset: 1, limit: 1, query: "") }"#,
        ))
        .await;
    assert_eq!(json_of(&resp)["appLogs"], serde_json::json!(["mid-line"]));

    // Negative args clamp instead of erroring (limit<=0 → empty page).
    let resp = schema
        .execute(async_graphql::Request::new(
            r#"{ appLogs(offset: -3, limit: -5, query: "") }"#,
        ))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    assert_eq!(json_of(&resp)["appLogs"], serde_json::json!([]));

    // The DSL `text:` field is a case-insensitive substring over the line,
    // applied before offset/limit (plain-app server-side `text:` extraction).
    let resp = schema
        .execute(async_graphql::Request::new(
            r#"{ appLogs(offset: 0, limit: 10, query: "text:MID") }"#,
        ))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    assert_eq!(json_of(&resp)["appLogs"], serde_json::json!(["mid-line"]));
    let resp = schema
        .execute(async_graphql::Request::new(
            r#"{ appLogs(offset: 0, limit: 10, query: "text:-line") }"#,
        ))
        .await;
    assert_eq!(
        json_of(&resp)["appLogs"],
        serde_json::json!(["new-line", "mid-line", "old-line"])
    );
    // A needle that matches nothing → empty, paginated or not.
    let resp = schema
        .execute(async_graphql::Request::new(
            r#"{ appLogs(offset: 5, limit: 2, query: "text:zzz") }"#,
        ))
        .await;
    assert_eq!(json_of(&resp)["appLogs"], serde_json::json!([]));
}

/// `deviceInfo` serves the plain-app contract (the shape the web
/// deviceInfoFragment selects): LINUX platform, top-level nullable
/// cpuModel, no android/display. Dynamic state lives in `deviceStatus`
/// (seconds uptime, battery null on a battery-less NAS). `sims` is not
/// part of the NAS schema at all — the web (nas branch) document no
/// longer selects it.
#[tokio::test]
async fn device_info_and_status_serve_plain_app_contract() {
    let (schema, _data_dir, _db, _prefs, _dir) = developer_schema();

    let resp = schema
        .execute(async_graphql::Request::new(
            r#"query {
                deviceInfo {
                    name platform osName osVersion cpuArch cpuModel
                    totalMemory totalStorage
                    display { width }
                    android { sdkVersion }
                }
                deviceStatus {
                    uptimeSec batteryLevel charging
                    temperatures { label celsius }
                    cpuUsage memoryAvailable storageAvailable
                }
            }"#,
        ))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    let json = json_of(&resp);

    let info = &json["deviceInfo"];
    assert_eq!(info["platform"], "LINUX");
    assert_eq!(info["osName"], "Linux");
    assert!(
        info["name"].as_str().is_some_and(|n| !n.is_empty()),
        "name falls back to hostname: {info}"
    );
    assert!(info["totalStorage"].is_i64(), "totalStorage: {info}");
    assert_eq!(info["android"], serde_json::Value::Null);
    assert_eq!(info["display"], serde_json::Value::Null);
    // cpuModel is a nullable top-level field — null when the host has no
    // model string (dev Macs), a string on real DMI-bearing hardware.
    assert!(
        info["cpuModel"].is_string() || info["cpuModel"].is_null(),
        "{info}"
    );

    let st = &json["deviceStatus"];
    assert!(st["uptimeSec"].is_i64(), "uptime seconds: {st}");
    assert!(st["cpuUsage"].is_f64(), "cpu usage percent: {st}");
    assert!(st["storageAvailable"].is_i64(), "storage available: {st}");
    // A NAS has no battery: level null, charging false.
    assert_eq!(st["batteryLevel"], serde_json::Value::Null);
    assert_eq!(st["charging"], false);
    assert!(st["temperatures"].is_array(), "{st}");
}

/// The removed surface stays removed (no compat layer): uptime on
/// DeviceInfo, the desktop sub-object, the battery query family, and the
/// pruned android extras must not come back.
#[tokio::test]
async fn legacy_device_surface_is_gone() {
    let (schema, _data_dir, _db, _prefs, _dir) = developer_schema();
    for q in [
        "{ deviceInfo { uptime } }",
        "{ deviceInfo { desktop { hostname } } }",
        "{ battery { level } }",
        "{ deviceInfo { android { buildHost product serial } } }",
    ] {
        let resp = schema.execute(async_graphql::Request::new(q)).await;
        assert!(!resp.errors.is_empty(), "query must be rejected: {q}");
    }
}

/// `deviceInfo.name` follows the `App.deviceName` precedence: the stored
/// display-name override wins; deleting it falls back to the hostname.
#[tokio::test]
async fn device_info_name_prefers_display_name_override() {
    let (schema, _data_dir, _db, _prefs, _dir) = developer_schema();

    let resp = schema
        .execute(
            async_graphql::Request::new(
                r#"mutation { updateDeviceName(name: "  dev box  ") }"#.to_string(),
            )
            .data("test-client".to_string()),
        )
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);

    let resp = schema
        .execute(async_graphql::Request::new("{ deviceInfo { name } }"))
        .await;
    assert_eq!(json_of(&resp)["deviceInfo"]["name"], "dev box");
}


/// `audioPlayback` on the phone contract: `currentPath` is null when idle
/// (the empty string is never sent), and the NAS — no server-side
/// transport — serves idle `isPlaying`/`positionMs` (API_SPEC §9). A
/// current track makes `currentPath` non-null without changing transport.
#[tokio::test]
async fn audio_playback_serves_nullable_path_and_idle_transport() {
    let dir = tempfile::tempdir().unwrap();
    let db = std::sync::Arc::new(crate::db::Db::open(dir.path()).unwrap());
    let config = std::sync::Arc::new(crate::config::Config::parse(
        "[server]\nhttp_port = 8080\nhttps_port = 8443\n",
    ));
    let schema = crate::gql::build_schema(
        db.clone(),
        prefs(&dir),
        config,
        dir.path().to_path_buf(),
        crate::test_support::chat_state(dir.path()),
    );

    let resp = schema
        .execute(async_graphql::Request::new(
            "{ audioPlayback { currentPath isPlaying positionMs } }",
        ))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    assert_eq!(
        resp.data.into_json().unwrap(),
        serde_json::json!({ "audioPlayback": {
            "currentPath": null,
            "isPlaying": false,
            "positionMs": 0,
        } })
    );

    let library = plain_rs::library::db::LibraryDb::open(&dir.path().join("library.db")).unwrap();
    plain_rs::library::audio_queue::save_audio_current(&library, "/music/a.mp3");
    let resp = schema
        .execute(async_graphql::Request::new(
            "{ audioPlayback { currentPath isPlaying positionMs } }",
        ))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    assert_eq!(
        resp.data.into_json().unwrap(),
        serde_json::json!({ "audioPlayback": {
            "currentPath": "/music/a.mp3",
            "isPlaying": false,
            "positionMs": 0,
        } })
    );
}

/// Round-12 contract: `pathStat`/`pathStats` are gone; the predicate pair
/// `pathExists` (Boolean!, total) + `pathKind` (FILE/DIR, null = missing)
/// answers the same questions without the wrapper type.
#[tokio::test]
async fn path_predicates_replace_path_stat() {
    let base = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let dir = tempfile::Builder::new()
        .prefix("plainnas-test-")
        .tempdir_in(base)
        .unwrap();
    let db = std::sync::Arc::new(crate::db::Db::open(dir.path()).unwrap());
    let config = std::sync::Arc::new(crate::config::Config::parse(
        "[server]\nhttp_port = 8080\nhttps_port = 8443\n",
    ));
    let schema = crate::gql::build_schema(
        db,
        prefs(&dir),
        config,
        dir.path().to_path_buf(),
        crate::test_support::chat_state(dir.path()),
    );
    std::fs::write(dir.path().join("a.txt"), b"x").unwrap();

    let resp = schema
        .execute(async_graphql::Request::new(
            r#"{ dirExists: pathExists(path: "/tmp")
                 dirKind: pathKind(path: "/tmp")
                 fileExists: pathExists(path: "/etc/hostname")
                 missingExists: pathExists(path: "/no/such/path")
                 missingKind: pathKind(path: "/no/such/path")
                 blankExists: pathExists(path: "   ") }"#,
        ))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    let d = resp.data.into_json().unwrap();
    assert_eq!(d["dirExists"], true);
    assert_eq!(d["dirKind"], "DIR");
    assert_eq!(d["missingExists"], false);
    assert_eq!(d["missingKind"], serde_json::Value::Null);
    assert_eq!(d["blankExists"], false);
    // FILE case: /etc/hostname exists on Linux and macOS; skip asserting
    // its kind where the fixture file may not (checked below instead).
    let resp = schema
        .execute(async_graphql::Request::new(format!(
            r#"{{ fileKind: pathKind(path: "{}") }}"#,
            dir.path()
                .join("a.txt")
                .to_str()
                .unwrap()
                .replace('"', "\\\"")
        )))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    assert_eq!(
        resp.data.into_json().unwrap()["fileKind"],
        "FILE",
        "a regular file must report FILE"
    );
}
