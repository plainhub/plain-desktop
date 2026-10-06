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

/// Any host call is a bug for these tests: the whole point is that tags are
/// Rust SQLite rows, so nothing should reach the platform.
fn fixture() -> (tempfile::TempDir, PublicSchema) {
    fixture_with(|method, _| panic!("unexpected host call {method}"))
}

fn fixture_with(
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

fn database(schema: &PublicSchema) -> &Arc<Db> {
    schema.data::<Arc<Db>>().unwrap()
}

/// The ordinals are plain-app's `DataType.value`, stored in the tag tables.
/// They are not a free ordering — changing one would re-file every existing
/// tag, so the contract enum and this mapping are pinned.
#[test]
fn content_kinds_keep_plain_apps_ordinals() {
    assert_eq!(DataType::Default.kind(), 0);
    assert_eq!(DataType::Audio.kind(), 1);
    assert_eq!(DataType::Video.kind(), 2);
    assert_eq!(DataType::Image.kind(), 3);
    assert_eq!(DataType::Sms.kind(), 4);
    assert_eq!(DataType::Contact.kind(), 5);
    assert_eq!(DataType::Note.kind(), 6);
    assert_eq!(DataType::FeedEntry.kind(), 7);
    assert_eq!(DataType::Call.kind(), 8);
    assert_eq!(DataType::Package.kind(), 21);
    assert_eq!(DataType::File.kind(), 22);
    assert_eq!(DataType::AppFile.kind(), 23);
    assert_eq!(DataType::Doc.kind(), 24);
}

#[tokio::test]
async fn a_created_tag_reads_back_with_its_kind_implied_not_stored() {
    let (_dir, schema) = fixture();
    let response = schema
        .execute(r#"mutation { createTag(type:IMAGE, name:"holiday") { id name count } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let created = response.data.into_json().unwrap();
    let id = created["createTag"]["id"].as_str().unwrap().to_string();
    assert_eq!(created["createTag"]["name"], "holiday");
    assert_eq!(created["createTag"]["count"], 0);

    let response = schema
        .execute(&format!(
            r#"query {{ tags(type:IMAGE) {{ id name count }} audio: tags(type:AUDIO) {{ id }} }}"#
        ))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["tags"].as_array().unwrap().len(), 1);
    assert_eq!(data["tags"][0]["id"], id.as_str());
    // The kind filter is real: an audio list does not see an image tag.
    assert_eq!(data["audio"], json!([]));
}

#[tokio::test]
async fn an_unknown_tag_id_is_null_rather_than_an_error() {
    let (_dir, schema) = fixture();
    let response = schema.execute(r#"query { tag(id:"nope") { id } }"#).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(response.data.into_json().unwrap()["tag"], Value::Null);
}

#[tokio::test]
async fn updating_an_unknown_tag_errors_instead_of_inventing_one() {
    let (_dir, schema) = fixture();
    let response = schema
        .execute(r#"mutation { updateTag(id:"nope", name:"x") { id } }"#)
        .await;
    assert_eq!(response.errors.len(), 1);
    assert!(response.errors[0].message.contains("not found"));
}

#[tokio::test]
async fn deleting_a_tag_drops_its_relations_too() {
    let (_dir, schema) = fixture();
    let response = schema
        .execute(r#"mutation { createTag(type:DOC, name:"pdf") { id } }"#)
        .await;
    let id = response.data.into_json().unwrap()["createTag"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let response = schema
        .execute(&format!(
            r#"mutation {{ updateTagRelations(type:DOC, item:{{key:"d1", title:"a", size:10}},
                              addTagIds:["{id}"], removeTagIds:[]) }}"#
        ))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let response = schema.execute(r#"query { tagKeys(id:"IGNORED") } "#).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);

    let response = schema
        .execute(&format!(r#"mutation {{ deleteTag(id:"{id}") }}"#))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let response = schema
        .execute(&format!(r#"query {{ keys: tagKeys(id:"{id}") }}"#))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(response.data.into_json().unwrap()["keys"], json!([]));
}

#[tokio::test]
async fn relations_are_listed_for_the_requested_keys_and_kind() {
    let (_dir, schema) = fixture();
    let response = schema
        .execute(
            r#"mutation { a: createTag(type:VIDEO, name:"v") { id }
                             b: createTag(type:VIDEO, name:"w") { id } }"#,
        )
        .await;
    let data = response.data.into_json().unwrap();
    let a = data["a"]["id"].as_str().unwrap().to_string();
    let b = data["b"]["id"].as_str().unwrap().to_string();
    let response = schema
        .execute(&format!(
            r#"mutation {{ one: updateTagRelations(type:VIDEO, item:{{key:"k1", title:"t", size:1}},
                                    addTagIds:["{a}"], removeTagIds:[])
                              two: updateTagRelations(type:VIDEO, item:{{key:"k2", title:"t", size:1}},
                                    addTagIds:["{a}"], removeTagIds:[])
                              rel: updateTagRelations(type:VIDEO, item:{{key:"k1", title:"t", size:1}},
                                    addTagIds:["{b}"], removeTagIds:["{a}"]) }}"#
        ))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);

    let response = schema
        .execute(r#"query { tagRelations(type:VIDEO, keys:["k1","k2","k3"]) { tagId key } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let rows = response.data.into_json().unwrap();
    let rows = rows["tagRelations"].as_array().unwrap();
    // k3 was never touched, so it contributes nothing.
    assert_eq!(rows.len(), 2);
    // The last mutation moved k1 from a to b; k2 still has a.
    let k1 = rows
        .iter()
        .find(|row| row["key"] == "k1")
        .expect("k1 relation");
    let k2 = rows
        .iter()
        .find(|row| row["key"] == "k2")
        .expect("k2 relation");
    assert_eq!(k1["tagId"], b.as_str());
    assert_eq!(k2["tagId"], a.as_str());
}

/// A relation may only point at a tag of the same kind — the library
/// rejects a mismatch rather than filing an image tag under a video.
#[tokio::test]
async fn a_relation_cannot_cross_kinds() {
    let (_dir, schema) = fixture();
    let response = schema
        .execute(r#"mutation { createTag(type:IMAGE, name:"i") { id } }"#)
        .await;
    let id = response.data.into_json().unwrap()["createTag"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let response = schema
        .execute(&format!(
            r#"mutation {{ updateTagRelations(type:VIDEO, item:{{key:"k1", title:"t", size:1}},
                              addTagIds:["{id}"], removeTagIds:[]) }}"#
        ))
        .await;
    assert_eq!(response.errors.len(), 1);
    assert!(response.errors[0].message.contains("tag type mismatch"));
    let response = schema
        .execute(r#"query { tagRelations(type:VIDEO, keys:["k1"]) { key } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["tagRelations"],
        json!([])
    );
}

#[tokio::test]
async fn adding_a_tag_to_a_query_attaches_it_to_every_match() {
    let (_dir, schema) = fixture_with(|method, params| match method {
        "systemTagQueryStubs" => {
            assert_eq!(params["dataType"], "DOC");
            assert_eq!(params["query"], "text:report");
            json!([
                {"key": "d1", "title": "a.pdf", "size": 10},
                {"key": "d2", "title": "b.pdf", "size": 20},
            ])
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"mutation { createTag(type:DOC, name:"work") { id } }"#)
        .await;
    let id = response.data.into_json().unwrap()["createTag"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let response = schema
        .execute(&format!(
            r#"mutation {{ addToTags(type:DOC, tagIds:["{id}"], query:"text:report") }}"#
        ))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let response = schema
        .execute(&format!(r#"query {{ tagKeys(id:"{id}") }}"#))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let keys = response.data.into_json().unwrap()["tagKeys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|key| key.as_str().unwrap().to_string())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        keys,
        ["d1".to_string(), "d2".to_string()].into_iter().collect()
    );
}

#[tokio::test]
async fn removing_a_tag_takes_it_off_every_match() {
    let (_dir, schema) = fixture_with(|method, params| match method {
        "systemTagQueryStubs" => json!([{"key": "d1", "title": "a", "size": 1}]),
        "systemTagQueryKeys" => {
            assert_eq!(params["dataType"], "DOC");
            json!({ "ids": ["d1", "d2"] })
        }
        other => panic!("unexpected host call {other}"),
    });
    let response = schema
        .execute(r#"mutation { createTag(type:DOC, name:"work") { id } }"#)
        .await;
    let id = response.data.into_json().unwrap()["createTag"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    for query in [
        r#"mutation { updateTagRelations(type:DOC, item:{key:"d1", title:"a", size:1}, addTagIds:["ID"], removeTagIds:[]) }"#,
        r#"mutation { updateTagRelations(type:DOC, item:{key:"d2", title:"b", size:1}, addTagIds:["ID"], removeTagIds:[]) }"#,
    ] {
        let response = schema.execute(&query.replace("ID", &id)).await;
        assert!(response.errors.is_empty(), "{:?}", response.errors);
    }
    let response = schema
        .execute(&format!(
            r#"mutation {{ removeFromTags(type:DOC, tagIds:["{id}"], query:"text:x") }}"#
        ))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let response = schema
        .execute(&format!(r#"query {{ tagKeys(id:"{id}") }}"#))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(response.data.into_json().unwrap()["tagKeys"], json!([]));
}

/// A blank query matches every row of the kind. Refusing it is the only
/// thing standing between a client bug and tagging the whole library.
#[tokio::test]
async fn a_blank_query_is_refused_before_the_platform_is_touched() {
    let (_dir, schema) =
        fixture_with(|method, _| panic!("the platform must not be asked: {method}"));
    let response = schema
        .execute(r#"mutation { createTag(type:DOC, name:"work") { id } }"#)
        .await;
    let id = response.data.into_json().unwrap()["createTag"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    for document in [
        format!(r#"mutation {{ addToTags(type:DOC, tagIds:["{id}"], query:"") }}"#),
        format!(r#"mutation {{ removeFromTags(type:DOC, tagIds:["{id}"], query:"   ") }}"#),
    ] {
        let response = schema.execute(&document).await;
        assert_eq!(response.errors.len(), 1);
        assert_eq!(response.errors[0].message, "explicit query required");
    }
}

/// NOTE and FEED_ENTRY resolve their ids through the notes and feeds roots.
/// A tag write must not re-enter the public schema to reach them, so those
/// kinds are refused until those roots land in Rust.
#[tokio::test]
async fn note_and_feed_entry_tag_writes_are_refused() {
    let (_dir, schema) =
        fixture_with(|method, _| panic!("the platform must not be asked: {method}"));
    for kind in ["NOTE", "FEED_ENTRY"] {
        let response = schema
            .execute(&format!(
                r#"mutation {{ addToTags(type:{kind}, tagIds:["t1"], query:"text:x") }}"#
            ))
            .await;
        assert_eq!(response.errors.len(), 1, "{kind}");
        assert!(
            response.errors[0]
                .message
                .starts_with("Unsupported tag query type"),
            "{}",
            response.errors[0].message
        );
    }
}

#[tokio::test]
async fn the_contract_tag_type_carries_no_numeric_kind() {
    // The list it came from implies the kind, so a `type` field would be a
    // second, disagreeing source for the same fact.
    let (_dir, schema) = fixture();
    let response = schema
        .execute(r#"mutation { createTag(type:IMAGE, name:"x") { id name count } }"#)
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    let tag = &data["createTag"];
    assert!(tag.get("type").is_none(), "{tag}");
}
