//! Tests for `src/media/gql/` — the shared media GraphQL roots: schema
//! assembly, the files listing, and the lazy-`tags` contract.
use std::sync::Arc;

use async_graphql::{EmptySubscription, Schema};

use crate::db::Db as SqlDb;
use crate::http_server::main_schemas::media::{MediaMutationRoot, MediaQueryRoot};
use crate::media::image_index::MediaSearchIndex;
use crate::media::kv::Db;
use crate::media::paths;
use crate::media::scan::MediaFile;
use crate::prefs::Prefs;

type MediaSchema = Schema<MediaQueryRoot, MediaMutationRoot, EmptySubscription>;

fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("plain-rs-media-gql-{tag}-{nanos}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn test_schema(tag: &str) -> (MediaSchema, std::path::PathBuf) {
    let data_dir = paths::pin_test_data_dir();
    let tmp = tmp_dir(tag);
    let db = Arc::new(Db::open(&tmp.join("fjall")).unwrap());
    let prefs = Arc::new(Prefs::load(&tmp.join("prefs.json")).unwrap());
    let library = Arc::new(SqlDb::open(&tmp.join("plain.db")).unwrap());
    let schema = Schema::build(MediaQueryRoot, MediaMutationRoot, EmptySubscription)
        .data(db)
        .data(prefs)
        .data(library)
        .data(String::new())
        .finish();
    (schema, data_dir)
}

fn audio_row(path: &str) -> MediaFile {
    MediaFile {
        uuid: format!("uuid-{}", path),
        fsuuid: String::new(),
        ino: 0,
        ctime: 0,
        duration_sec: 0,
        duration_ref_mod: 0,
        duration_ref_size: 0,
        artist: String::new(),
        artist_ref_mod: 0,
        artist_ref_size: 0,
        title: String::new(),
        title_ref_mod: 0,
        title_ref_size: 0,
        path: path.to_string(),
        original_path: String::new(),
        name: format!("{path}.mp3"),
        size: 10,
        modified_at: 1000,
        r#type: "audio".to_string(),
        is_trash: false,
        trash_path: String::new(),
        deleted_at: 0,
    }
}

#[test]
fn sdl_contains_the_media_surface() {
    let (schema, _) = test_schema("sdl");
    let sdl = schema.sdl();
    for needle in [
        "fileCount(root: String!, query: String!): Int!",
        "files(",
        "mediaBuckets(",
        "startMediaScan(root: String!): Boolean!",
        "enum FileSortBy",
        "type Audio",
        "type Image",
        "type Video",
        "type Doc",
        "type TrashItem",
        "type FileTask",
        "type ScanProgress",
        "enum MediaDataType",
    ] {
        assert!(sdl.contains(needle), "SDL missing {needle}");
    }
}

#[tokio::test]
async fn media_list_tags_are_lazy_loaded() {
    let (schema, _data_dir) = test_schema("lazy");
    let idx: Arc<MediaSearchIndex> = crate::media::image_index::global();
    idx.clear().unwrap();
    idx.add_media_file(&audio_row("lazy-audio")).unwrap();
    idx.commit().unwrap();

    use std::sync::atomic::Ordering;
    crate::http_server::main_schemas::media::TAG_LOADS.store(0, Ordering::Relaxed);
    let resp = schema
        .execute(async_graphql::Request::new(
            r#"{ audios(offset: 0, limit: 10, query: "text:lazy-audio", sortBy: DATE_DESC) { id } }"#,
        ))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    assert_eq!(
        crate::http_server::main_schemas::media::TAG_LOADS.load(Ordering::Relaxed),
        0,
        "no tags selection must not touch the tag store"
    );

    crate::http_server::main_schemas::media::TAG_LOADS.store(0, Ordering::Relaxed);
    let resp = schema
        .execute(async_graphql::Request::new(
            r#"{ audios(offset: 0, limit: 10, query: "text:lazy-audio", sortBy: DATE_DESC) { id tags { id } } }"#,
        ))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    let data = resp.data.into_json().unwrap();
    let rows = data["audios"].as_array().map(Vec::len).unwrap_or(0);
    assert_eq!(rows, 1, "expected exactly our fixture row: {data}");
    assert_eq!(
        crate::http_server::main_schemas::media::TAG_LOADS.load(Ordering::Relaxed),
        rows,
        "tags selection loads exactly once per row"
    );

    idx.clear().unwrap();
    idx.commit().unwrap();
}

#[tokio::test]
async fn path_predicates_never_raise() {
    let (schema, _) = test_schema("paths");
    let resp = schema
        .execute(async_graphql::Request::new(
            r#"{ pathExists(path: ".") pathKind(path: "/definitely/not/here") }"#,
        ))
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    let data = resp.data.into_json().unwrap();
    assert_eq!(data["pathExists"], serde_json::json!(false));
    assert_eq!(data["pathKind"], serde_json::json!(null));
}

#[tokio::test]
async fn bulk_media_mutation_rejects_blank_query() {
    let (schema, _) = test_schema("bulk");
    let resp = schema
        .execute(async_graphql::Request::new(
            r#"mutation { trashMediaItems(type: AUDIO, query: "") { affectedCount } }"#,
        ))
        .await;
    assert!(!resp.errors.is_empty(), "blank query must be rejected");
    let msg = format!("{:?}", resp.errors);
    assert!(msg.contains("bulk_query_required"), "{msg}");
}
#[test]
fn default_scan_root_uses_all_configured_media_sources() {
    let sources = vec![
        "/Users/alex/Pictures".to_string(),
        "/Users/alex/Music".to_string(),
    ];
    assert_eq!(
        super::selected_scan_roots("/".to_string(), &sources),
        vec![
            std::path::PathBuf::from("/Users/alex/Pictures"),
            std::path::PathBuf::from("/Users/alex/Music")
        ]
    );
    assert_eq!(
        super::selected_scan_roots("/tmp/sample".to_string(), &sources),
        vec![std::path::PathBuf::from("/tmp/sample")]
    );
    assert_eq!(
        super::selected_scan_roots("/".to_string(), &[]),
        vec![std::path::PathBuf::from("/")]
    );
}
