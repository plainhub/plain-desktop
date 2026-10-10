use super::*;
fn facts() -> Facts {
    Facts {
        id: "peer".into(),
        name: "name ' 中文".into(),
        ips: vec!["127.0.0.1".into(), "".into(), "127.0.0.1".into()],
        port: 2443,
        device_type: DeviceType::Phone,
        key: crate::base64_encode(&[1; 32]),
        public_key: crate::base64_encode(&[2; 32]),
    }
}
#[test]
fn repairing_preserves_creation_and_independent_token_and_rolls_back_errors() {
    let db = Db::open(std::path::Path::new(":memory:")).unwrap();
    let first = save(&db, facts()).unwrap();
    assert_eq!(first.ip, "127.0.0.1");
    db.with_conn(|c| {
        c.execute(
            "UPDATE peers SET token='keep',created_at='2000-01-01T00:00:00Z' WHERE id='peer'",
            [],
        )
    })
    .unwrap();
    let mut next = facts();
    next.key = crate::base64_encode(&[3; 32]);
    let updated = save(&db, next).unwrap();
    assert_eq!(updated.token, "keep");
    assert_eq!(updated.created_at, "2000-01-01T00:00:00Z");
    assert_eq!(updated.key, crate::base64_encode(&[3; 32]));
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER fail_pair BEFORE UPDATE ON peers BEGIN SELECT RAISE(ABORT,'fixture failure'); END;")).unwrap();
    assert!(save(&db, facts()).is_err());
    assert_eq!(db.get_peer_by_id("peer").unwrap().key, updated.key);
    let mut invalid = facts();
    invalid.key = "bad".into();
    assert!(save(&db, invalid).is_err());
}
