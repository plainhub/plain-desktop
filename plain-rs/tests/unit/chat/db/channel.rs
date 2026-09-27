use super::*;
use crate::chat::db::tests::unique_tmp_dir;
use crate::chat::enums::ChannelStatus;

fn seed_channel(db: &ChatDb, id: &str, status: ChannelStatus) {
    let mut channel = DChannel::new(id, "me");
    channel.id = id.to_string();
    channel.status = status;
    db.insert_channel(&channel);
}

#[test]
fn get_channels_filters_by_status() {
    let db = ChatDb::open(&unique_tmp_dir("filter-status").join("local_chat.db")).expect("open db");
    seed_channel(&db, "c1", ChannelStatus::Joined);
    seed_channel(&db, "c2", ChannelStatus::Joined);
    seed_channel(&db, "c3", ChannelStatus::Left);

    let joined: Vec<String> = db
        .get_channels(ChannelStatus::Joined)
        .into_iter()
        .map(|c| c.id)
        .collect();
    assert_eq!(joined, vec!["c1".to_string(), "c2".to_string()]);

    let left: Vec<String> = db
        .get_channels(ChannelStatus::Left)
        .into_iter()
        .map(|c| c.id)
        .collect();
    assert_eq!(left, vec!["c3".to_string()]);
}

#[test]
fn joined_member_ids_filters_by_joined_status() {
    let mut ch = DChannel::new("test", "owner");
    ch.members = r#"[{"id":"p1","status":"JOINED"},{"id":"p2","status":"PENDING"},{"id":"p3","status":"JOINED"}]"#.to_string();

    let ids = ch.joined_member_ids();
    assert_eq!(ids, vec!["p1".to_string(), "p3".to_string()]);
}

#[test]
fn elect_leader_prefers_owner_then_smallest_online() {
    use std::collections::HashSet;
    let mut ch = DChannel::new("test", "owner-1");
    ch.members = r#"[{"peerId":"owner-1","status":"JOINED"},{"peerId":"b","status":"JOINED"},{"peerId":"c","status":"JOINED"}]"#.to_string();

    let online: HashSet<String> = ["b".to_string(), "c".to_string()].into_iter().collect();
    assert_eq!(ch.elect_leader(&online, "me"), Some("b".to_string()));

    let online: HashSet<String> = ["owner-1".to_string(), "c".to_string()]
        .into_iter()
        .collect();
    assert_eq!(ch.elect_leader(&online, "me"), Some("owner-1".to_string()));

    let empty: HashSet<String> = HashSet::new();
    assert_eq!(ch.elect_leader(&empty, "me"), None);
}

#[test]
fn any_channel_has_member_returns_true_when_member_found() {
    let db = ChatDb::open(&unique_tmp_dir("has-member").join("local_chat.db")).expect("open db");
    let mut ch = DChannel::new("ch1", "owner");
    ch.members = r#"[{"id":"peer1","status":"JOINED"}]"#.to_string();
    ch.status = ChannelStatus::Joined;
    db.insert_channel(&ch);

    assert!(db.any_channel_has_member("peer1"));
    assert!(!db.any_channel_has_member("peer2"));
}
