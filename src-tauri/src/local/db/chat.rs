use rusqlite::params;

use super::ChatDb;
use super::utils::{now_iso, short_id};
use crate::local::enums::ChatStatus;

#[derive(Clone, Debug)]
pub struct DChat {
    pub id: String,
    pub from_id: String,
    pub to_id: String,
    pub channel_id: String,
    pub content: String,
    pub status: ChatStatus,
    pub status_data: String,
    pub created_at: String,
    pub updated_at: String,
}

impl DChat {
    pub fn new(from_id: &str, to_id: &str, channel_id: &str, content: &str) -> Self {
        let now = now_iso();
        Self {
            id: short_id(),
            from_id: from_id.to_string(),
            to_id: to_id.to_string(),
            channel_id: channel_id.to_string(),
            content: content.to_string(),
            status: ChatStatus::Sent,
            status_data: String::new(),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

impl ChatDb {
    /// Target-scoped chat page (plain-app `chatItems` contract): `target`
    /// is `channel:<id>` or a bare/`peer:`-prefixed peer id, `text` is a
    /// substring filter on `content`, results oldest-first.
    pub fn get_chats_page(&self, id: &str, text: &str, offset: i32, limit: i32) -> Vec<DChat> {
        let conn = self.0.lock().unwrap();
        let text = text.trim();
        let (base_sql, p1): (&str, &str) = if let Some(cid) = id.strip_prefix("channel:") {
            ("WHERE channel_id=?1", cid)
        } else {
            let pid = id.strip_prefix("peer:").unwrap_or(id);
            ("WHERE channel_id='' AND (to_id=?1 OR from_id=?1)", pid)
        };
        let text_clause = if text.is_empty() {
            String::new()
        } else {
            format!(" AND content LIKE '%{}%'", text.replace('\'', "''"))
        };        let sql = format!(
            "SELECT id,from_id,to_id,channel_id,content,status,status_data,created_at,updated_at \
             FROM chats {base_sql}{text_clause} ORDER BY created_at DESC LIMIT ?2 OFFSET ?3"
        );

        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return vec![],
        };

        let row_to_chat = |row: &rusqlite::Row<'_>| -> rusqlite::Result<DChat> {
            Ok(DChat {
                id: row.get(0)?,
                from_id: row.get(1)?,
                to_id: row.get(2)?,
                channel_id: row.get(3)?,
                content: row.get(4)?,
                status: row.get(5)?,
                status_data: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        };

        let rows = stmt.query_map(params![p1, limit, offset], row_to_chat);
        let mut chats: Vec<DChat> = rows
            .ok()
            .map(|iter| iter.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
        chats.reverse();
        chats
    }

    pub fn insert_chat(&self, chat: &DChat) {
        let conn = self.0.lock().unwrap();
        let _ = conn.execute(
            "INSERT INTO chats (id,from_id,to_id,channel_id,content,status,status_data,created_at,updated_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                chat.id, chat.from_id, chat.to_id, chat.channel_id,
                chat.content, chat.status, chat.status_data,
                chat.created_at, chat.updated_at
            ],
        );
    }

    pub fn delete_chat(&self, id: &str) -> bool {
        let conn = self.0.lock().unwrap();
        conn.execute("DELETE FROM chats WHERE id=?", params![id])
            .map(|n| n > 0)
            .unwrap_or(false)
    }

    pub fn update_chat_status(&self, id: &str, status: ChatStatus) -> Option<DChat> {
        let now = now_iso();
        {
            let conn = self.0.lock().unwrap();
            let _ = conn.execute(
                "UPDATE chats SET status=?1, updated_at=?2 WHERE id=?3",
                params![status, now, id],
            );
        }
        self.get_chat_by_id(id)
    }

    pub fn update_chat_status_and_data(
        &self,
        id: &str,
        status: ChatStatus,
        status_data: &str,
    ) -> Option<DChat> {
        let now = now_iso();
        {
            let conn = self.0.lock().unwrap();
            let _ = conn.execute(
                "UPDATE chats SET status=?1, status_data=?2, updated_at=?3 WHERE id=?4",
                params![status, status_data, now, id],
            );
        }
        self.get_chat_by_id(id)
    }

    pub fn get_chat_by_id(&self, id: &str) -> Option<DChat> {
        let conn = self.0.lock().unwrap();
        conn.query_row(
            "SELECT id,from_id,to_id,channel_id,content,status,status_data,created_at,updated_at \
             FROM chats WHERE id=?",
            params![id],
            |row| {
                Ok(DChat {
                    id: row.get(0)?,
                    from_id: row.get(1)?,
                    to_id: row.get(2)?,
                    channel_id: row.get(3)?,
                    content: row.get(4)?,
                    status: row.get(5)?,
                    status_data: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            },
        )
        .ok()
    }

    pub fn update_chat_content(&self, id: &str, content: &str) -> bool {
        let now = now_iso();
        let conn = self.0.lock().unwrap();
        conn.execute(
            "UPDATE chats SET content=?1, updated_at=?2 WHERE id=?3",
            params![content, now, id],
        )
        .map(|n| n > 0)
        .unwrap_or(false)
    }

    pub fn get_all_chats(&self) -> Vec<DChat> {
        let conn = self.0.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id,from_id,to_id,channel_id,content,status,status_data,created_at,updated_at \
             FROM chats ORDER BY created_at ASC",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![], |row| {
            Ok(DChat {
                id: row.get(0)?,
                from_id: row.get(1)?,
                to_id: row.get(2)?,
                channel_id: row.get(3)?,
                content: row.get(4)?,
                status: row.get(5)?,
                status_data: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })
        .ok()
        .map(|iter| iter.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    /// Direct translation of plain-app `ChatDao.getAllLatestChats()`:
    /// ```kotlin
    /// @Query("""
    ///     SELECT c.* FROM chats c
    ///     INNER JOIN (
    ///         SELECT '' as from_id, '' as to_id, channel_id, MAX(created_at) as max_created_at
    ///         FROM chats WHERE channel_id != '' GROUP BY channel_id
    ///         UNION ALL
    ///         SELECT from_id, to_id, '' as channel_id, MAX(created_at) as max_created_at
    ///         FROM chats WHERE channel_id = '' GROUP BY from_id, to_id
    ///     ) latest ON (
    ///         (c.channel_id != '' AND c.channel_id = latest.channel_id AND c.created_at = latest.max_created_at)
    ///         OR (c.channel_id = '' AND c.from_id = latest.from_id AND c.to_id = latest.to_id AND c.created_at = latest.max_created_at)
    ///     )
    ///     ORDER BY c.created_at DESC
    /// """)
    /// suspend fun getAllLatestChats(): List<DChat>
    /// ```
    pub fn get_all_latest_chats(&self) -> Vec<DChat> {
        let conn = self.0.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT c.id,c.from_id,c.to_id,c.channel_id,c.content,c.status,c.status_data,c.created_at,c.updated_at \
             FROM chats c \
             INNER JOIN ( \
                 SELECT '' as from_id, '' as to_id, channel_id, MAX(created_at) as max_created_at \
                 FROM chats WHERE channel_id != '' GROUP BY channel_id \
                 UNION ALL \
                 SELECT from_id, to_id, '' as channel_id, MAX(created_at) as max_created_at \
                 FROM chats WHERE channel_id = '' GROUP BY from_id, to_id \
             ) latest ON ( \
                 (c.channel_id != '' AND c.channel_id = latest.channel_id AND c.created_at = latest.max_created_at) \
                 OR (c.channel_id = '' AND c.from_id = latest.from_id AND c.to_id = latest.to_id AND c.created_at = latest.max_created_at) \
             ) \
             ORDER BY c.created_at DESC",
        ) {
            Ok(s) => s,
            Err(e) => {
                log::error!("[chat] get_all_latest_chats prepare error: {e}");
                return vec![];
            }
        };
        match stmt.query_map(params![], |row| {
            Ok(DChat {
                id: row.get(0)?,
                from_id: row.get(1)?,
                to_id: row.get(2)?,
                channel_id: row.get(3)?,
                content: row.get(4)?,
                status: row.get(5)?,
                status_data: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        }) {
            Ok(iter) => iter
                .filter_map(|r| {
                    r.map_err(|e| log::error!("[chat] get_all_latest_chats row error: {e}"))
                        .ok()
                })
                .collect(),
            Err(e) => {
                log::error!("[chat] get_all_latest_chats query_map error: {e}");
                vec![]
            }
        }
    }

    pub fn delete_chats_by_channel(&self, channel_id: &str) {
        let conn = self.0.lock().unwrap();
        let _ = conn.execute("DELETE FROM chats WHERE channel_id=?", params![channel_id]);
    }

    pub fn delete_chats_by_peer(&self, peer_id: &str) {
        let conn = self.0.lock().unwrap();
        let _ = conn.execute(
            "DELETE FROM chats WHERE channel_id='' AND (to_id=? OR from_id=?)",
            params![peer_id, peer_id],
        );
    }

    pub fn delete_chats_by_ids(&self, ids: &[String]) {
        if ids.is_empty() {
            return;
        }
        let conn = self.0.lock().unwrap();
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!("DELETE FROM chats WHERE id IN ({placeholders})");
        let params: Vec<&dyn rusqlite::ToSql> =
            ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
        let _ = conn.execute(&sql, params.as_slice());
    }

    pub fn get_chats_by_peer(&self, peer_id: &str) -> Vec<DChat> {
        let conn = self.0.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id,from_id,to_id,channel_id,content,status,status_data,created_at,updated_at \
             FROM chats WHERE channel_id='' AND (to_id=? OR from_id=?) ORDER BY created_at ASC",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![peer_id, peer_id], |row| {
            Ok(DChat {
                id: row.get(0)?,
                from_id: row.get(1)?,
                to_id: row.get(2)?,
                channel_id: row.get(3)?,
                content: row.get(4)?,
                status: row.get(5)?,
                status_data: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })
        .ok()
        .map(|iter| iter.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    pub fn get_chats_by_channel(&self, channel_id: &str) -> Vec<DChat> {
        let conn = self.0.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id,from_id,to_id,channel_id,content,status,status_data,created_at,updated_at \
             FROM chats WHERE channel_id=? ORDER BY created_at ASC",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![channel_id], |row| {
            Ok(DChat {
                id: row.get(0)?,
                from_id: row.get(1)?,
                to_id: row.get(2)?,
                channel_id: row.get(3)?,
                content: row.get(4)?,
                status: row.get(5)?,
                status_data: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })
        .ok()
        .map(|iter| iter.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_tmp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let pid = std::process::id();
        std::env::temp_dir().join(format!("plainapp-chat-{label}-{pid}-{nanos}"))
    }

    fn seed_chat(db: &ChatDb, id: &str, from_id: &str, to_id: &str, channel_id: &str) {
        let mut chat = DChat::new(from_id, to_id, channel_id, "{}");
        chat.id = id.to_string();
        db.insert_chat(&chat);
    }

    #[test]
    fn get_chats_by_peer_returns_both_directions() {
        let db = ChatDb::open(&unique_tmp_dir("peer-both").join("local_chat.db")).expect("open db");
        seed_chat(&db, "a", "me", "peer1", "");
        seed_chat(&db, "b", "peer1", "me", "");
        seed_chat(&db, "c", "me", "peer2", "");
        seed_chat(&db, "d", "me", "peer1", "ch1");

        let ids: Vec<String> = db
            .get_chats_by_peer("peer1")
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn delete_chats_by_peer_preserves_channel_chats() {
        let db = ChatDb::open(&unique_tmp_dir("peer-del").join("local_chat.db")).expect("open db");
        seed_chat(&db, "a", "me", "peer1", "");
        seed_chat(&db, "b", "peer1", "me", "");
        seed_chat(&db, "c", "me", "peer1", "ch1");

        db.delete_chats_by_peer("peer1");

        assert!(db.get_chat_by_id("a").is_none());
        assert!(db.get_chat_by_id("b").is_none());
        assert!(db.get_chat_by_id("c").is_some());
    }

    #[test]
    fn delete_chats_by_ids_removes_only_listed_ids() {
        let db = ChatDb::open(&unique_tmp_dir("ids-del").join("local_chat.db")).expect("open db");
        seed_chat(&db, "a", "me", "p", "");
        seed_chat(&db, "b", "me", "p", "");
        seed_chat(&db, "c", "me", "p", "");

        db.delete_chats_by_ids(&["a".to_string(), "c".to_string()]);

        assert!(db.get_chat_by_id("a").is_none());
        assert!(db.get_chat_by_id("b").is_some());
        assert!(db.get_chat_by_id("c").is_none());
    }

    #[test]
    fn delete_chats_by_ids_noop_for_empty_list() {
        let db = ChatDb::open(&unique_tmp_dir("ids-empty").join("local_chat.db")).expect("open db");
        seed_chat(&db, "a", "me", "p", "");
        db.delete_chats_by_ids(&[]);
        assert!(db.get_chat_by_id("a").is_some());
    }

    #[test]
    fn get_chats_by_channel_returns_only_that_channel() {
        let db = ChatDb::open(&unique_tmp_dir("chan-get").join("local_chat.db")).expect("open db");
        seed_chat(&db, "a", "me", "", "ch1");
        seed_chat(&db, "b", "me", "", "ch2");
        seed_chat(&db, "c", "me", "", "ch1");

        let ids: Vec<String> = db
            .get_chats_by_channel("ch1")
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(ids, vec!["a".to_string(), "c".to_string()]);
    }

    #[test]
    fn get_all_latest_chats_returns_local_chat() {
        let db =
            ChatDb::open(&unique_tmp_dir("latest-local").join("local_chat.db")).expect("open db");
        seed_chat(&db, "a", "me", "local", "");

        let latest = db.get_all_latest_chats();
        assert_eq!(
            latest.len(),
            1,
            "should return 1 latest chat, got {latest:?}"
        );
        assert_eq!(latest[0].from_id, "me");
        assert_eq!(latest[0].to_id, "local");
    }

    #[test]
    fn get_all_latest_chats_returns_peer_and_channel() {
        let db =
            ChatDb::open(&unique_tmp_dir("latest-mixed").join("local_chat.db")).expect("open db");
        seed_chat(&db, "a", "me", "local", "");
        seed_chat(&db, "b", "me", "peer1", "");
        seed_chat(&db, "c", "peer1", "me", "");
        seed_chat(&db, "d", "me", "", "ch1");

        let latest = db.get_all_latest_chats();
        // 4 conversations: me↔local, me→peer1, peer1→me, ch1
        assert_eq!(
            latest.len(),
            4,
            "should return 4 latest chats, got {latest:?}"
        );
    }

    #[test]
    fn get_all_latest_chats_returns_empty_when_no_chats() {
        let db =
            ChatDb::open(&unique_tmp_dir("latest-empty").join("local_chat.db")).expect("open db");
        let latest = db.get_all_latest_chats();
        assert!(latest.is_empty());
    }
}
