use super::*;
use async_graphql::Request;

#[tokio::test]
async fn graphql_note_lifecycle_and_tags() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(LibraryDb::open(&dir.path().join("library.db")).unwrap());
    let schema = crate::httpserver::mainschemas::build_schema();
    let response = schema.execute(Request::new(r#"mutation { createNote(input: { title: "Title", content: "needle" }) { id title tags { id } } }"#).data(db.clone())).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let id = response.data.into_json().unwrap()["createNote"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let tag = crate::library::tags::create_tag(&db, DataType::Note.kind(), "work").unwrap();
    crate::library::tags::add_relations(&db, &[(tag.id, id.clone())]);
    let response = schema.execute(Request::new(format!(r#"{{ note(id: "{id}") {{ title content tags {{ name }} }} noteCount(query: "text:needle") }}"#)).data(db.clone())).await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    let data = response.data.into_json().unwrap();
    assert_eq!(data["note"]["tags"][0]["name"], "work");
    assert_eq!(data["noteCount"], 1);
    let response = schema
        .execute(
            Request::new(r#"mutation { trashNotes(query: "all:true") { affectedCount } }"#)
                .data(db.clone()),
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["trashNotes"]["affectedCount"],
        1
    );
    let response = schema
        .execute(
            Request::new(
                r#"{ notes(offset: 0, limit: 10, query: "trash:true") { id deletedAt } }"#,
            )
            .data(db),
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(response.data.into_json().unwrap()["notes"][0]["id"], id);
}
