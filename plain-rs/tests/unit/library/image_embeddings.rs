use super::*;
#[path = "fixtures.rs"]
mod fixtures;
use fixtures::test_db;
fn encode(values: &[f32]) -> String {
    crate::utils::base64::base64_encode(&
        values
            .iter()
            .flat_map(|v| v.to_be_bytes())
            .collect::<Vec<_>>(),
    )
}
fn item(id: &str, values: &[f32]) -> EmbeddingInput {
    EmbeddingInput {
        id: id.into(),
        path: format!("/synthetic/{id}"),
        embedding_base64: encode(values),
    }
}
#[test]
fn ranked_search_is_bounded_and_persisted() {
    let db = test_db("embeddings_search");
    save(
        &db,
        &[
            item("c", &[0.1, 0.0]),
            item("b", &[0.9, 0.0]),
            item("a", &[0.9, 0.0]),
            item("best", &[1.0, 0.0]),
        ],
    )
    .unwrap();
    let results = search(&db, &encode(&[1.0, 0.0]), 2).unwrap();
    assert_eq!(
        results
            .iter()
            .map(|r| r.image_id.as_str())
            .collect::<Vec<_>>(),
        ["best", "a"]
    );
    assert_eq!(results[0].score, 1.0);
    assert_eq!(count(&db).unwrap(), 4);
    assert_eq!(delete(&db, &[]).unwrap(), 0);
    assert_eq!(delete(&db, &["best".into()]).unwrap(), 1);
    assert_eq!(search(&db, &encode(&[1.0, 0.0]), 10).unwrap().len(), 2);
    assert_eq!(clear(&db).unwrap(), 3);
}
#[test]
fn invalid_vectors_and_failed_batch_do_not_commit_partial_updates() {
    let db = test_db("embeddings_validation");
    save(&db, &[item("old", &[1.0, 0.0])]).unwrap();
    assert!(
        save(
            &db,
            &[item("new", &[1.0, 0.0]), item("bad", &[f32::NAN, 0.0])]
        )
        .is_err()
    );
    assert_eq!(ids(&db).unwrap(), ["old"]);
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER fail_embedding BEFORE INSERT ON image_embeddings WHEN NEW.id='bad' BEGIN SELECT RAISE(ABORT,'fail'); END")).unwrap();
    assert!(save(&db, &[item("new", &[1.0, 0.0]), item("bad", &[1.0, 0.0])]).is_err());
    assert_eq!(ids(&db).unwrap(), ["old"]);
    assert!(search(&db, &encode(&[1.0]), 5).is_err());
    assert!(search(&db, &encode(&[f32::INFINITY, 0.0]), 5).is_err());
    assert!(search(&db, "invalid", 5).is_err());
    assert!(search(&db, &encode(&[1.0, 0.0]), 501).is_err());
}
