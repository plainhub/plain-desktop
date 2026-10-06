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

/// `granted` is the platform's runtime answer for WRITE_EXTERNAL_STORAGE.
fn fixture(permissions: &str, granted: bool) -> (tempfile::TempDir, PublicSchema) {
    fixture_with(permissions, granted, move |method, _| match method {
        "systemPermissionFacts" => json!({ "granted": { "WRITE_EXTERNAL_STORAGE": granted } }),
        other => panic!("unexpected host call {other}"),
    })
}

fn fixture_with(
    permissions: &str,
    granted: bool,
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
    stub(host.clone(), move |method, params| {
        if method == "systemPermissionFacts" {
            return json!({ "granted": { "WRITE_EXTERNAL_STORAGE": granted } });
        }
        handler(method, params)
    });
    (
        dir,
        crate::content_api::public_schema::build(host, prefs, db),
    )
}

fn image_row(id: &str, path: &str) -> Value {
    json!({
        "id": id, "title": id, "path": path, "size": 2048,
        "width": 4000, "height": 3000, "rotation": 0, "bucketId": "b1",
        "createdAt": "2026-01-02T03:04:05Z", "updatedAt": "2026-02-03T04:05:06Z",
        "takenAt": "2026-01-01T00:00:00Z", "isFavorite": true,
    })
}

/// The contract `Tag` has no numeric kind field — the media kind is implied
/// by the list it came from.
fn tag(id: &str, name: &str) -> Value {
    json!({ "id": id, "name": name, "count": 7 })
}

/// Counts degrade to zero and folder strips degrade to empty when the
/// platform grant is missing: the web client shows an empty library, not an
/// error page.
#[tokio::test]
async fn counts_and_buckets_degrade_when_storage_is_not_granted() {
    let (_dir, schema) = fixture(GRANTED, false);
    let response = schema
        .execute(
            r#"query { imageCount(query:"") videoCount(query:"") docCount(query:"")
                      mediaBuckets(type:IMAGE) { id } docExtGroups { ext } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["imageCount"], 0);
    assert_eq!(data["videoCount"], 0);
    assert_eq!(data["docCount"], 0);
    assert_eq!(data["mediaBuckets"], json!([]));
    assert_eq!(data["docExtGroups"], json!([]));
}

/// The same degradation applies without the web-client opt-in at all.
#[tokio::test]
async fn counts_degrade_without_the_api_permission_too() {
    let (_dir, schema) = fixture("[]", true);
    let response = schema
        .execute(r#"query { imageCount(query:"") mediaBuckets(type:VIDEO) { id } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["imageCount"], 0);
    assert_eq!(data["mediaBuckets"], json!([]));
}

/// Browsing is not a soft operation — without the opt-in it errors, so a
/// client cannot mistake "you may not look" for "there is nothing there".
#[tokio::test]
async fn browsing_media_errors_out_without_the_api_permission() {
    let (_dir, schema) = fixture("[]", true);
    let response = schema
        .execute(
            r#"query { images(offset:0, limit:10, query:"", sortBy:NAME_ASC) { id }
                      videos(offset:0, limit:10, query:"", sortBy:NAME_ASC) { id }
                      docs(offset:0, limit:10, query:"", sortBy:NAME_ASC) { id } }"#,
        )
        .await;
    assert_eq!(response.errors.len(), 3);
    for error in &response.errors {
        assert_eq!(error.message, "no_permission");
    }
}

#[tokio::test]
async fn images_carry_their_row_and_their_tags() {
    let (_dir, schema) = fixture_with(GRANTED, true, |method, params| match method {
        "systemImageRows" => {
            assert_eq!(params["offset"], 0);
            assert_eq!(params["limit"], 20);
            assert_eq!(params["sortBy"], "DATE_DESC");
            json!([
                image_row("i1", "/storage/DCIM/one.jpg"),
                image_row("i2", "/storage/DCIM/two.jpg"),
            ])
        }
        "systemMediaTagFacts" => {
            assert_eq!(params["dataType"], "IMAGE");
            assert_eq!(params["keys"], json!(["i1", "i2"]));
            json!([
                {"key": "i1", "tags": [tag("t1", "holiday")]},
                {"key": "i2", "tags": []},
            ])
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"query { images(offset:0, limit:20, query:"", sortBy:DATE_DESC)
                      { id title path size bucketId createdAt updatedAt takenAt isFavorite
                        tags { id name count } } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    let items = data["images"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["id"], "i1");
    assert_eq!(items[0]["bucketId"], "b1");
    assert_eq!(items[0]["size"], 2048);
    assert_eq!(items[0]["isFavorite"], true);
    assert_eq!(items[0]["takenAt"], "2026-01-01T00:00:00.000Z");
    assert_eq!(items[0]["tags"].as_array().unwrap().len(), 1);
    assert_eq!(items[0]["tags"][0]["name"], "holiday");
    assert_eq!(items[0]["tags"][0]["count"], 7);
    // An item with no relations reports an empty list, not null — clients
    // spread this straight into a chip row.
    assert_eq!(items[1]["tags"], json!([]));
}

/// `text` is split off the query and sent separately so the platform does
/// not filter on it twice; everything else rides through as `extraQuery`.
#[tokio::test]
async fn the_text_filter_is_split_out_of_the_image_query() {
    let (_dir, schema) = fixture_with(GRANTED, true, |method, params| match method {
        "systemImageRows" => {
            assert_eq!(params["queryText"], "sunset");
            assert_eq!(params["extraQuery"], "text:sunset size:>100");
            json!([])
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"query { images(offset:0, limit:5, query:"text:sunset size:>100", sortBy:NAME_ASC) { id } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
}

#[tokio::test]
async fn a_query_without_a_text_field_sends_an_empty_text() {
    let (_dir, schema) = fixture_with(GRANTED, true, |method, params| match method {
        "systemImageRows" => {
            assert_eq!(params["queryText"], "");
            assert_eq!(params["extraQuery"], "size:>100");
            json!([])
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"query { images(offset:0, limit:5, query:"size:>100", sortBy:NAME_ASC) { id } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
}

#[tokio::test]
async fn the_image_count_uses_the_combined_search() {
    let (_dir, schema) = fixture_with(GRANTED, true, |method, params| match method {
        "systemImageCount" => {
            assert_eq!(params["queryText"], "cat");
            assert_eq!(params["extraQuery"], "text:cat");
            json!(42)
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"query { imageCount(query:"text:cat") }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(response.data.into_json().unwrap()["imageCount"], 42);
}

#[tokio::test]
async fn videos_carry_the_duration_and_the_taken_time() {
    let (_dir, schema) = fixture_with(GRANTED, true, |method, params| match method {
        "systemMediaRows" => {
            assert_eq!(params["dataType"], "VIDEO");
            assert_eq!(params["sortBy"], "SIZE_ASC");
            json!([{
                "id": "v1", "title": "clip", "path": "/storage/Movies/clip.mp4",
                "durationMs": 61000, "size": 900, "width": 1920, "height": 1080,
                "rotation": 0, "bucketId": "b2", "createdAt": "2026-01-02T03:04:05Z",
                "updatedAt": "2026-02-03T04:05:06Z", "takenAt": Value::Null,
                "isFavorite": false,
            }])
        }
        "systemMediaTagFacts" => json!([]),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"query { videos(offset:0, limit:10, query:"", sortBy:SIZE_ASC)
                      { id durationMs size takenAt tags { id } } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["videos"][0]["durationMs"], 61000);
    assert_eq!(data["videos"][0]["takenAt"], Value::Null);
    // No tag rows at all still yields an empty list per item.
    assert_eq!(data["videos"][0]["tags"], json!([]));
}

/// `Doc.extension` is derived from the path, matching plain-app's lazy
/// property — the media store does not carry an extension column.
#[tokio::test]
async fn doc_extensions_come_from_the_path() {
    let (_dir, schema) = fixture_with(GRANTED, true, |method, _| match method {
        "systemMediaRows" => json!([
            {"id":"d1","title":"a","path":"/storage/Docs/a.pdf","durationMs":0,"size":10,
             "bucketId":"b3","createdAt":"2026-01-02T03:04:05Z","updatedAt":"2026-01-02T03:04:05Z"},
            {"id":"d2","title":"b","path":"/storage/Docs/noext","durationMs":0,"size":10,
             "bucketId":"b3","createdAt":"2026-01-02T03:04:05Z","updatedAt":"2026-01-02T03:04:05Z"},
        ]),
        "systemMediaTagFacts" => json!([]),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(
            r#"query { docs(offset:0, limit:10, query:"", sortBy:NAME_ASC) { id extension } }"#,
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["docs"][0]["extension"], "pdf");
    assert_eq!(data["docs"][1]["extension"], "");
}

/// One row per folder, sorted by the lowercased folder name, with a bounded
/// number of sample paths for the thumbnail grid.
#[tokio::test]
async fn buckets_collapse_rows_into_folders() {
    let (_dir, schema) = fixture_with(GRANTED, true, |method, params| match method {
        "systemMediaBucketItemFacts" => {
            assert_eq!(params["dataType"], "VIDEO");
            json!([
                {"id":"b2","name":"Movies","size":10,"path":"/storage/Movies/a.mp4","sortName":"Movies"},
                {"id":"b1","name":"Camera","size":10,"path":"/storage/Camera/a.jpg","sortName":"Camera"},
                {"id":"b2","name":"Movies","size":10,"path":"/storage/Movies/b.mp4","sortName":"Movies"},
                {"id":"b2","name":"Movies","size":10,"path":"/storage/Movies/c.mp4","sortName":"Movies"},
                {"id":"b2","name":"Movies","size":10,"path":"/storage/Movies/d.mp4","sortName":"Movies"},
                {"id":"b2","name":"Movies","size":10,"path":"/storage/Movies/e.mp4","sortName":"Movies"},
            ])
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"query { mediaBuckets(type:VIDEO) { id name itemCount topItemPaths } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    let buckets = data["mediaBuckets"].as_array().unwrap();
    assert_eq!(buckets.len(), 2);
    // Camera sorts before Movies.
    assert_eq!(buckets[0]["id"], "b1");
    assert_eq!(buckets[0]["itemCount"], 1);
    assert_eq!(buckets[1]["id"], "b2");
    assert_eq!(buckets[1]["itemCount"], 5);
    // Five rows in, but only four samples out — the grid never needs more.
    assert_eq!(buckets[1]["topItemPaths"].as_array().unwrap().len(), 4);
}

#[tokio::test]
async fn doc_extension_groups_are_forwarded_verbatim() {
    let (_dir, schema) = fixture_with(GRANTED, true, |method, _| match method {
        "systemDocExtGroups" => json!([{"ext": "pdf", "count": 12}, {"ext": "epub", "count": 3}]),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"query { docExtGroups { ext count } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    let groups = data["docExtGroups"].as_array().unwrap();
    assert_eq!(groups[0]["ext"], "pdf");
    assert_eq!(groups[0]["count"], 12);
    assert_eq!(groups[1]["ext"], "epub");
}

#[tokio::test]
async fn video_and_doc_counts_reach_the_platform_with_the_kind() {
    let (_dir, schema) = fixture_with(GRANTED, true, |method, params| match method {
        "systemMediaCount" => {
            assert!(params["dataType"] == "VIDEO" || params["dataType"] == "DOC");
            json!(5)
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"query { videoCount(query:"") docCount(query:"size:>10") }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["videoCount"], 5);
    assert_eq!(data["docCount"], 5);
}

/// An empty page must not cost a tag round trip.
#[tokio::test]
async fn an_empty_page_skips_the_tag_lookup() {
    let (_dir, schema) = fixture_with(GRANTED, true, |method, _| match method {
        "systemImageRows" => json!([]),
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"query { images(offset:0, limit:10, query:"", sortBy:NAME_ASC) { id } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(response.data.into_json().unwrap()["images"], json!([]));
}

#[test]
fn the_contract_kind_names_are_what_the_platform_expects() {
    assert_eq!(MediaDataType::Audio.as_str(), "AUDIO");
    assert_eq!(MediaDataType::Video.as_str(), "VIDEO");
    assert_eq!(MediaDataType::Image.as_str(), "IMAGE");
    assert_eq!(MediaDataType::Doc.as_str(), "DOC");
}
