use rusqlite::params;

use crate::chat::channel::messages::decode_members;
use crate::chat::enums::ChannelStatus;
use crate::db::{DChannel, Db};

pub(crate) const CHANNEL_COLS: &str = "id,name,owner_id,members,key,version,status,created_at,updated_at";

pub(crate) fn row_to_channel(row: &rusqlite::Row<'_>) -> rusqlite::Result<DChannel> {
    Ok(DChannel {
        id: row.get(0)?,
        name: row.get(1)?,
        owner_id: row.get(2)?,
        members: row.get(3)?,
        key: row.get(4)?,
        version: row.get::<_, i64>(5)?,
        status: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

impl Db {
    pub fn get_channels(&self, status: ChannelStatus) -> Vec<DChannel> {
        let conn = self.0.lock().unwrap();
        let mut stmt = match conn.prepare(&format!(
            "SELECT {CHANNEL_COLS} FROM chat_channels WHERE status=? ORDER BY name ASC"
        )) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![status], row_to_channel)
            .ok()
            .map(|iter| iter.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    /// Return all channels regardless of status. Mirrors Kotlin's
    /// `chatChannelDao().getAll()` used by `ChatCacher.load()`.
    pub fn get_all_channels(&self) -> Vec<DChannel> {
        let conn = self.0.lock().unwrap();
        let mut stmt = match conn.prepare(&format!(
            "SELECT {CHANNEL_COLS} FROM chat_channels ORDER BY name ASC"
        )) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map([], row_to_channel)
            .ok()
            .map(|iter| iter.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    pub fn get_channels_with_key(&self) -> Vec<DChannel> {
        let conn = self.0.lock().unwrap();
        let mut stmt = match conn.prepare(&format!(
            "SELECT {CHANNEL_COLS} FROM chat_channels WHERE key != ''"
        )) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map([], row_to_channel)
            .ok()
            .map(|iter| iter.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    }

    pub fn get_channel_by_id(&self, id: &str) -> Option<DChannel> {
        let conn = self.0.lock().unwrap();
        conn.query_row(
            &format!("SELECT {CHANNEL_COLS} FROM chat_channels WHERE id=?"),
            params![id],
            row_to_channel,
        )
        .ok()
    }

    pub fn insert_channel(&self, channel: &DChannel) {
        let conn = self.0.lock().unwrap();
        let _ = conn.execute(
            "INSERT INTO chat_channels (id,name,owner_id,members,key,version,status,created_at,updated_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                channel.id, channel.name, channel.owner_id, channel.members,
                channel.key, channel.version, channel.status,
                channel.created_at, channel.updated_at
            ],
        );
    }

    pub fn update_channel(&self, channel: &DChannel) {
        let conn = self.0.lock().unwrap();
        let _ = conn.execute(
            "UPDATE chat_channels SET name=?1,owner_id=?2,members=?3,key=?4,version=?5,status=?6,updated_at=?7 WHERE id=?8",
            params![
                channel.name, channel.owner_id, channel.members, channel.key,
                channel.version, channel.status, channel.updated_at, channel.id
            ],
        );
    }

    pub fn upsert_channel(&self, channel: &DChannel) {
        let conn = self.0.lock().unwrap();
        let _ = conn.execute(
            "INSERT INTO chat_channels (id,name,owner_id,members,key,version,status,created_at,updated_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9) \
             ON CONFLICT(id) DO UPDATE SET \
               name=excluded.name, owner_id=excluded.owner_id, members=excluded.members, \
               key=excluded.key, version=excluded.version, status=excluded.status, \
               updated_at=excluded.updated_at",
            params![
                channel.id, channel.name, channel.owner_id, channel.members,
                channel.key, channel.version, channel.status,
                channel.created_at, channel.updated_at
            ],
        );
    }

    pub fn delete_channel(&self, id: &str) {
        let conn = self.0.lock().unwrap();
        let _ = conn.execute("DELETE FROM chat_channels WHERE id=?", params![id]);
    }

    pub fn any_channel_has_member(&self, peer_id: &str) -> bool {
        let conn = self.0.lock().unwrap();
        let mut stmt = match conn.prepare("SELECT members FROM chat_channels WHERE status=?") {
            Ok(s) => s,
            Err(_) => return false,
        };
        let mut rows = match stmt.query(params![ChannelStatus::Joined]) {
            Ok(r) => r,
            Err(_) => return false,
        };
        while let Ok(Some(row)) = rows.next() {
            let members_json: String = row.get(0).unwrap_or_default();
            if decode_members(&members_json)
                .iter()
                .any(|m| m.peer_id == peer_id)
            {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/chat/db/channel.rs"]
mod tests;
