use super::*;
use crate::db::chat_store::{SaveMode, messages};
fn fixture() -> (Db, DChat, Card) {
    let db = Db::open(std::path::Path::new(":memory:")).unwrap();
    let card:Card=serde_json::from_value(serde_json::json!({"shareId":"fixture","urlToken":"token","peerInfo":{"id":"peer","ip":"old","port":443},"name":"old","itemCount":3,"totalSize":42,"expiresAt":"2030-01-01T00:00:00Z"})).unwrap();
    let mut value = serde_json::to_value(&card).unwrap();
    value["futureMetadata"] = serde_json::json!("preserved");
    let row = DChat::new(
        "peer",
        "me",
        "group",
        &serde_json::json!({"type":"SHARE","value":value}).to_string(),
    );
    messages::save(&db, &[row.clone()], SaveMode::Insert).unwrap();
    (db, row, card)
}
#[test]
fn refresh_is_atomic_preserves_message_state_and_clears_removed_expiry() {
    let (db, row, expected) = fixture();
    messages::status(
        &db,
        &row.id,
        crate::chat::enums::ChatStatus::Sent,
        Some("{\"results\":[]}"),
    )
    .unwrap();
    let (card, updated) = refresh(
        &db,
        &row.id,
        &expected,
        &row.content,
        "::1",
        2443,
        "new",
        None,
    )
    .unwrap();
    assert_eq!(card.expires_at, None);
    assert_eq!(card.item_count, 3);
    assert_eq!(card.total_size, 42);
    let updated = updated.unwrap();
    assert_eq!(updated.channel_id, row.channel_id);
    assert_eq!(updated.created_at, row.created_at);
    assert_eq!(updated.updated_at, row.updated_at);
    assert_eq!(updated.status_data, "{\"results\":[]}");
    let value: Value = serde_json::from_str(&updated.content).unwrap();
    assert_eq!(value["value"]["futureMetadata"], "preserved");
    assert!(
        refresh(&db, &row.id, &card, &updated.content, "::1", 2443, "", None)
            .unwrap()
            .1
            .is_none()
    );
    assert!(
        refresh(
            &db,
            &row.id,
            &expected,
            &row.content,
            "other",
            443,
            "late",
            None
        )
        .is_err()
    );
    assert_eq!(
        messages::get(&db, &row.id).unwrap().unwrap().content,
        updated.content
    );
}
#[test]
fn replaced_deleted_and_failed_updates_never_overwrite_or_publish() {
    let (db, row, expected) = fixture();
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER reject_refresh BEFORE UPDATE OF content ON chats BEGIN SELECT RAISE(ABORT,'fixture'); END;")).unwrap();
    assert!(
        refresh(
            &db,
            &row.id,
            &expected,
            &row.content,
            "new",
            443,
            "new",
            None
        )
        .is_err()
    );
    assert_eq!(
        messages::get(&db, &row.id).unwrap().unwrap().content,
        row.content
    );
    db.with_conn(|c| c.execute_batch("DROP TRIGGER reject_refresh;"))
        .unwrap();
    messages::content(
        &db,
        &row.id,
        "{\"type\":\"TEXT\",\"value\":{\"text\":\"replacement\"}}",
    )
    .unwrap();
    assert!(
        refresh(
            &db,
            &row.id,
            &expected,
            &row.content,
            "new",
            443,
            "new",
            None
        )
        .is_err()
    );
    messages::delete(&db, &[row.id.clone()]).unwrap();
    assert!(
        refresh(
            &db,
            &row.id,
            &expected,
            &row.content,
            "new",
            443,
            "new",
            None
        )
        .is_err()
    );
}
