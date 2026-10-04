use super::*;
use crate::db::chat_store::peers;
use crate::{chat::enums::DeviceType, db::DPeer};

#[test]
fn creation_mutations_and_invite_validation_preserve_credentials_and_version_rules() {
    let db = Db::open(std::path::Path::new(":memory:")).unwrap();
    let channel = create(&db, "owner", "  中文 group  ").unwrap();
    assert_eq!(channel.name, "中文 group");
    assert_eq!(channel.version, 1);
    assert_eq!(crate::base64_decode(&channel.key).len(), 32);
    assert_eq!(
        serde_json::from_str::<Vec<ChannelMember>>(&channel.members).unwrap(),
        vec![ChannelMember::new("owner")]
    );
    assert!(apply(&db, "owner", &channel.id, Action::Leave).is_err());
    assert!(
        apply(
            &db,
            "other",
            &channel.id,
            Action::Invite {
                peer: "guest".into()
            }
        )
        .is_err()
    );
    let invited = apply(
        &db,
        "owner",
        &channel.id,
        Action::Invite {
            peer: "guest".into(),
        },
    )
    .unwrap();
    assert_eq!(invited.version, 2);
    assert_eq!(invited.key, channel.key);
    assert_eq!(invited.created_at, channel.created_at);
    assert!(
        apply(
            &db,
            "owner",
            &channel.id,
            Action::Invite {
                peer: "guest".into()
            }
        )
        .is_err()
    );
    assert!(
        apply(
            &db,
            "owner",
            &channel.id,
            Action::Resend {
                peer: "guest".into()
            }
        )
        .is_err()
    );
    let owner = DPeer::new("owner", "owner", "", 1, DeviceType::Phone);
    let guest = DPeer::new("guest", "guest", "", 1, DeviceType::Phone);
    peers::save(&db, &[owner, guest], SaveMode::Insert).unwrap();
    let resent = apply(
        &db,
        "owner",
        &channel.id,
        Action::Resend {
            peer: "guest".into(),
        },
    )
    .unwrap();
    assert_eq!(resent.version, 2);
    assert_eq!(resent.updated_at, invited.updated_at);
    assert!(apply(&db, "other", &channel.id, Action::Accept).is_err());
    assert!(apply(&db, "owner", &channel.id, Action::Accept).is_err());
    assert_eq!(
        apply(&db, "guest", &channel.id, Action::Accept)
            .unwrap()
            .version,
        2
    );
    let renamed = apply(
        &db,
        "owner",
        &channel.id,
        Action::Rename {
            name: "  name %_ '  ".into(),
        },
    )
    .unwrap();
    assert_eq!(renamed.name, "name %_ '");
    assert_eq!(renamed.version, 3);
    assert!(
        apply(
            &db,
            "guest",
            &channel.id,
            Action::Kick {
                peer: "owner".into()
            }
        )
        .is_err()
    );
    let removed = apply(
        &db,
        "owner",
        &channel.id,
        Action::Kick {
            peer: "guest".into(),
        },
    )
    .unwrap();
    assert_eq!(removed.version, 4);
    assert_eq!(removed.key, channel.key);
    assert!(
        apply(
            &db,
            "owner",
            &channel.id,
            Action::Kick {
                peer: "guest".into()
            }
        )
        .is_err()
    );
    let left = apply(&db, "guest", &channel.id, Action::Leave).unwrap();
    assert_eq!(left.status, ChannelStatus::Left);
    assert_eq!(left.version, 4);
}

#[test]
fn sql_failure_and_version_overflow_leave_channel_state_unchanged() {
    let db = Db::open(std::path::Path::new(":memory:")).unwrap();
    let channel = create(&db, "owner", "before").unwrap();
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER reject_update BEFORE UPDATE ON chat_channels BEGIN SELECT RAISE(ABORT,'state rollback'); END;")).unwrap();
    assert!(
        apply(
            &db,
            "owner",
            &channel.id,
            Action::Invite {
                peer: "guest".into()
            }
        )
        .is_err()
    );
    let unchanged = channels::get(&db, &channel.id).unwrap().unwrap();
    assert_eq!(unchanged.members, channel.members);
    assert_eq!(unchanged.version, 1);
    assert_eq!(unchanged.updated_at, channel.updated_at);
    db.with_conn(|c| c.execute_batch("DROP TRIGGER reject_update"))
        .unwrap();
    db.with_conn(|c| {
        c.execute(
            "UPDATE chat_channels SET version=?2 WHERE id=?1",
            params![channel.id, i64::MAX],
        )
    })
    .unwrap();
    assert!(
        apply(
            &db,
            "owner",
            &channel.id,
            Action::Rename {
                name: "after".into()
            }
        )
        .is_err()
    );
    assert_eq!(
        channels::get(&db, &channel.id).unwrap().unwrap().name,
        "before"
    );
    assert!(apply(&db, "owner", "missing", Action::Leave).is_err());
    assert!(create(&db, "", "bad").is_err());
}

#[test]
fn concurrent_member_actions_read_current_state_and_do_not_lose_members() {
    let db = Db::open(std::path::Path::new(":memory:")).unwrap();
    let channel = create(&db, "owner", "concurrent").unwrap();
    let mut handles = Vec::new();
    for i in 0..12 {
        let db = db.clone();
        let id = channel.id.clone();
        handles.push(std::thread::spawn(move || {
            apply(
                &db,
                "owner",
                &id,
                Action::Invite {
                    peer: format!("peer-{i}"),
                },
            )
            .unwrap()
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
    let result = channels::get(&db, &channel.id).unwrap().unwrap();
    assert_eq!(result.version, 13);
    assert_eq!(
        serde_json::from_str::<Vec<ChannelMember>>(&result.members)
            .unwrap()
            .len(),
        13
    );
    assert_eq!(result.key, channel.key);
}
