use base64::Engine;
use rusqlite::types::Value;
use rusqlite::{OptionalExtension, params, params_from_iter};

use crate::db::Db;
use crate::db::models::simple::{
    ArchivedConversationRow, ClipboardRow, ImageEmbeddingRow, MediaItemRow, PomodoroItemRow,
    SessionRow, ShareRow, TrashedMessageRow, VideoPlayProgressRow,
};

// Re-exported so FFI consumers can name the row types without reaching into
// the private models module.
pub use crate::db::models::simple::*;

const CLIPBOARD_COLUMNS: &str = "id, text, hash, source, label, sensitive, created_at";
const SESSION_COLUMNS: &str = "client_id, name, type, client_ip, os_name, os_version, \
     browser_name, browser_version, token, last_active_at, created_at, updated_at";
const SHARE_COLUMNS: &str = "id, name, password, url_token, expires_at, read_only, data, \
     created_at, updated_at";
const POMODORO_COLUMNS: &str =
    "id, date, completed_count, total_work_seconds, total_break_seconds, created_at, updated_at";
const MEDIA_ITEM_COLUMNS: &str = "media_type, media_id, duration_ms, updated_at";
const VIDEO_PROGRESS_COLUMNS: &str = "media_id, position_ms, updated_at";
const EMBEDDING_COLUMNS: &str = "id, path, embedding, created_at, updated_at";

fn clipboard_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ClipboardRow> {
    Ok(ClipboardRow {
        id: row.get(0)?,
        text: row.get(1)?,
        hash: row.get(2)?,
        source: row.get(3)?,
        label: row.get(4)?,
        sensitive: row.get::<_, i64>(5)? != 0,
        created_at: row.get(6)?,
    })
}

fn session_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        client_id: row.get(0)?,
        name: row.get(1)?,
        r#type: row.get(2)?,
        client_ip: row.get(3)?,
        os_name: row.get(4)?,
        os_version: row.get(5)?,
        browser_name: row.get(6)?,
        browser_version: row.get(7)?,
        token: row.get(8)?,
        last_active_at: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn share_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ShareRow> {
    Ok(ShareRow {
        id: row.get(0)?,
        name: row.get(1)?,
        password: row.get(2)?,
        url_token: row.get(3)?,
        expires_at: row.get(4)?,
        read_only: row.get::<_, i64>(5)? != 0,
        data: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn pomodoro_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PomodoroItemRow> {
    Ok(PomodoroItemRow {
        id: row.get(0)?,
        date: row.get(1)?,
        completed_count: row.get(2)?,
        total_work_seconds: row.get(3)?,
        total_break_seconds: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn media_item_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MediaItemRow> {
    Ok(MediaItemRow {
        media_type: row.get(0)?,
        media_id: row.get(1)?,
        duration_ms: row.get(2)?,
        updated_at: row.get(3)?,
    })
}

fn video_progress_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<VideoPlayProgressRow> {
    Ok(VideoPlayProgressRow {
        media_id: row.get(0)?,
        position_ms: row.get(1)?,
        updated_at: row.get(2)?,
    })
}

fn embedding_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ImageEmbeddingRow> {
    let blob: Vec<u8> = row.get(2)?;
    Ok(ImageEmbeddingRow {
        id: row.get(0)?,
        path: row.get(1)?,
        embedding_base64: base64::engine::general_purpose::STANDARD.encode(blob),
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

fn in_clause(column: &str, values: &[String]) -> (String, Vec<Value>) {
    if values.is_empty() {
        return ("0=1".to_string(), Vec::new());
    }
    let placeholders = vec!["?"; values.len()].join(",");
    (
        format!("{column} IN ({placeholders})"),
        values.iter().map(|v| Value::Text(v.clone())).collect(),
    )
}

impl Db {
    pub fn clipboard_save(&self, row: &ClipboardRow) -> rusqlite::Result<()> {
        self.with_conn(|c| {
            c.execute(
                "INSERT OR REPLACE INTO clipboards (id, text, hash, source, label, sensitive, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    row.id,
                    row.text,
                    row.hash,
                    row.source,
                    row.label,
                    row.sensitive as i64,
                    row.created_at
                ],
            )?;
            Ok(())
        })
    }

    pub fn clipboard_get(&self, id: &str) -> rusqlite::Result<Option<ClipboardRow>> {
        self.with_conn(|c| {
            c.query_row(
                &format!("SELECT {CLIPBOARD_COLUMNS} FROM clipboards WHERE id=?1"),
                [id],
                clipboard_from_row,
            )
            .optional()
        })
    }

    pub fn clipboard_latest(&self) -> rusqlite::Result<Option<ClipboardRow>> {
        self.with_conn(|c| {
            c.query_row(
                &format!(
                    "SELECT {CLIPBOARD_COLUMNS} FROM clipboards ORDER BY created_at DESC LIMIT 1"
                ),
                [],
                clipboard_from_row,
            )
            .optional()
        })
    }

    pub fn clipboard_latest_by_hash(&self, hash: &str) -> rusqlite::Result<Option<ClipboardRow>> {
        self.with_conn(|c| {
            c.query_row(
                &format!(
                    "SELECT {CLIPBOARD_COLUMNS} FROM clipboards WHERE hash=?1 ORDER BY created_at DESC LIMIT 1"
                ),
                [hash],
                clipboard_from_row,
            )
            .optional()
        })
    }

    pub fn clipboard_delete_by_ids(&self, ids: &[String]) -> rusqlite::Result<usize> {
        let (clause, values) = in_clause("id", ids);
        self.with_conn(|c| {
            c.execute(
                &format!("DELETE FROM clipboards WHERE {clause}"),
                params_from_iter(values),
            )
        })
    }

    pub fn clipboard_clear(&self) -> rusqlite::Result<usize> {
        self.with_conn(|c| c.execute("DELETE FROM clipboards", []))
    }

    pub fn clipboard_count_rows(&self) -> rusqlite::Result<i64> {
        self.with_conn(|c| c.query_row("SELECT COUNT(*) FROM clipboards", [], |r| r.get(0)))
    }

    pub fn session_list(&self) -> rusqlite::Result<Vec<SessionRow>> {
        self.with_conn(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {SESSION_COLUMNS} FROM sessions ORDER BY last_active_at DESC"
            ))?;
            stmt.query_map([], session_from_row)?.collect()
        })
    }

    pub fn session_get(&self, client_id: &str) -> rusqlite::Result<Option<SessionRow>> {
        self.with_conn(|c| {
            c.query_row(
                &format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE client_id=?1"),
                [client_id],
                session_from_row,
            )
            .optional()
        })
    }

    pub fn session_save(&self, row: &SessionRow) -> rusqlite::Result<()> {
        self.with_conn(|c| {
            c.execute(
                "INSERT OR REPLACE INTO sessions (client_id, name, type, client_ip, os_name,
                     os_version, browser_name, browser_version, token, last_active_at,
                     created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    row.client_id,
                    row.name,
                    row.r#type,
                    row.client_ip,
                    row.os_name,
                    row.os_version,
                    row.browser_name,
                    row.browser_version,
                    row.token,
                    row.last_active_at,
                    row.created_at,
                    row.updated_at
                ],
            )?;
            Ok(())
        })
    }

    /// Bumps only `last_active_at` for the listed clients (the `updateTs` DAO).
    pub fn session_touch(&self, items: &[(String, String)]) -> rusqlite::Result<usize> {
        self.with_conn(|c| {
            let tx = c.unchecked_transaction()?;
            let mut updated = 0;
            {
                let mut stmt =
                    tx.prepare("UPDATE sessions SET last_active_at=?2 WHERE client_id=?1")?;
                for (client_id, at) in items {
                    updated += stmt.execute(params![client_id, at])?;
                }
            }
            tx.commit()?;
            Ok(updated)
        })
    }

    pub fn session_delete(&self, client_id: &str) -> rusqlite::Result<usize> {
        self.with_conn(|c| c.execute("DELETE FROM sessions WHERE client_id=?1", [client_id]))
    }

    pub fn share_list(&self) -> rusqlite::Result<Vec<ShareRow>> {
        self.with_conn(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {SHARE_COLUMNS} FROM shares ORDER BY created_at DESC"
            ))?;
            stmt.query_map([], share_from_row)?.collect()
        })
    }

    pub fn share_get(&self, id: &str) -> rusqlite::Result<Option<ShareRow>> {
        self.with_conn(|c| {
            c.query_row(
                &format!("SELECT {SHARE_COLUMNS} FROM shares WHERE id=?1"),
                [id],
                share_from_row,
            )
            .optional()
        })
    }

    pub fn share_save(&self, row: &ShareRow) -> rusqlite::Result<()> {
        self.with_conn(|c| {
            c.execute(
                "INSERT OR REPLACE INTO shares (id, name, password, url_token, expires_at,
                     read_only, data, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    row.id,
                    row.name,
                    row.password,
                    row.url_token,
                    row.expires_at,
                    row.read_only as i64,
                    row.data,
                    row.created_at,
                    row.updated_at
                ],
            )?;
            Ok(())
        })
    }

    pub fn share_delete(&self, id: &str) -> rusqlite::Result<usize> {
        self.with_conn(|c| c.execute("DELETE FROM shares WHERE id=?1", [id]))
    }

    pub fn pomodoro_list(&self) -> rusqlite::Result<Vec<PomodoroItemRow>> {
        self.with_conn(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {POMODORO_COLUMNS} FROM pomodoro_items ORDER BY date DESC"
            ))?;
            stmt.query_map([], pomodoro_from_row)?.collect()
        })
    }

    pub fn pomodoro_get_by_date(&self, date: &str) -> rusqlite::Result<Option<PomodoroItemRow>> {
        self.with_conn(|c| {
            c.query_row(
                &format!("SELECT {POMODORO_COLUMNS} FROM pomodoro_items WHERE date=?1"),
                [date],
                pomodoro_from_row,
            )
            .optional()
        })
    }

    pub fn pomodoro_recent(
        &self,
        start_date: &str,
        limit: i64,
    ) -> rusqlite::Result<Vec<PomodoroItemRow>> {
        self.with_conn(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {POMODORO_COLUMNS} FROM pomodoro_items WHERE date >= ?1
                 ORDER BY date DESC LIMIT ?2"
            ))?;
            stmt.query_map(params![start_date, limit.max(0)], pomodoro_from_row)?
                .collect()
        })
    }

    pub fn pomodoro_total_completed(&self) -> rusqlite::Result<i64> {
        self.with_conn(|c| {
            c.query_row("SELECT SUM(completed_count) FROM pomodoro_items", [], |r| {
                r.get::<_, Option<i64>>(0)
            })
            .map(|v| v.unwrap_or(0))
        })
    }

    pub fn pomodoro_save(&self, row: &PomodoroItemRow) -> rusqlite::Result<()> {
        self.with_conn(|c| {
            c.execute(
                "INSERT OR REPLACE INTO pomodoro_items (id, date, completed_count,
                     total_work_seconds, total_break_seconds, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    row.id,
                    row.date,
                    row.completed_count,
                    row.total_work_seconds,
                    row.total_break_seconds,
                    row.created_at,
                    row.updated_at
                ],
            )?;
            Ok(())
        })
    }

    pub fn pomodoro_delete(&self, id: &str) -> rusqlite::Result<usize> {
        self.with_conn(|c| c.execute("DELETE FROM pomodoro_items WHERE id=?1", [id]))
    }

    pub fn media_item_list(&self) -> rusqlite::Result<Vec<MediaItemRow>> {
        self.with_conn(|c| {
            let mut stmt = c.prepare(&format!("SELECT {MEDIA_ITEM_COLUMNS} FROM media_item"))?;
            stmt.query_map([], media_item_from_row)?.collect()
        })
    }

    pub fn media_item_upsert(&self, row: &MediaItemRow) -> rusqlite::Result<()> {
        self.with_conn(|c| {
            c.execute(
                "INSERT OR REPLACE INTO media_item (media_type, media_id, duration_ms, updated_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    row.media_type,
                    row.media_id,
                    row.duration_ms,
                    row.updated_at
                ],
            )?;
            Ok(())
        })
    }

    pub fn media_item_delete(&self, media_id: &str) -> rusqlite::Result<usize> {
        self.with_conn(|c| c.execute("DELETE FROM media_item WHERE media_id=?1", [media_id]))
    }

    pub fn video_progress_recent(
        &self,
        since: &str,
    ) -> rusqlite::Result<Vec<VideoPlayProgressRow>> {
        self.with_conn(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {VIDEO_PROGRESS_COLUMNS} FROM video_play_progress WHERE updated_at >= ?1"
            ))?;
            stmt.query_map([since], video_progress_from_row)?.collect()
        })
    }

    pub fn video_progress_get(
        &self,
        media_id: &str,
    ) -> rusqlite::Result<Option<VideoPlayProgressRow>> {
        self.with_conn(|c| {
            c.query_row(
                &format!(
                    "SELECT {VIDEO_PROGRESS_COLUMNS} FROM video_play_progress WHERE media_id=?1"
                ),
                [media_id],
                video_progress_from_row,
            )
            .optional()
        })
    }

    pub fn video_progress_upsert(&self, row: &VideoPlayProgressRow) -> rusqlite::Result<()> {
        self.with_conn(|c| {
            c.execute(
                "INSERT OR REPLACE INTO video_play_progress (media_id, position_ms, updated_at)
                 VALUES (?1, ?2, ?3)",
                params![row.media_id, row.position_ms, row.updated_at],
            )?;
            Ok(())
        })
    }

    pub fn video_progress_delete(&self, media_id: &str) -> rusqlite::Result<usize> {
        self.with_conn(|c| {
            c.execute(
                "DELETE FROM video_play_progress WHERE media_id=?1",
                [media_id],
            )
        })
    }

    pub fn embedding_list(&self) -> rusqlite::Result<Vec<ImageEmbeddingRow>> {
        self.with_conn(|c| {
            let mut stmt =
                c.prepare(&format!("SELECT {EMBEDDING_COLUMNS} FROM image_embeddings"))?;
            stmt.query_map([], embedding_from_row)?.collect()
        })
    }

    pub fn embedding_ids(&self) -> rusqlite::Result<Vec<String>> {
        self.with_conn(|c| {
            let mut stmt = c.prepare("SELECT id FROM image_embeddings")?;
            stmt.query_map([], |r| r.get(0))?.collect()
        })
    }

    pub fn embedding_count(&self) -> rusqlite::Result<i64> {
        self.with_conn(|c| c.query_row("SELECT COUNT(*) FROM image_embeddings", [], |r| r.get(0)))
    }

    pub fn embedding_save(&self, row: &ImageEmbeddingRow) -> rusqlite::Result<()> {
        let blob = base64::engine::general_purpose::STANDARD
            .decode(&row.embedding_base64)
            .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
        self.with_conn(|c| {
            c.execute(
                "INSERT OR REPLACE INTO image_embeddings (id, path, embedding, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![row.id, row.path, blob, row.created_at, row.updated_at],
            )?;
            Ok(())
        })
    }

    pub fn embedding_delete_all(&self) -> rusqlite::Result<usize> {
        self.with_conn(|c| c.execute("DELETE FROM image_embeddings", []))
    }

    pub fn embedding_delete_by_ids(&self, ids: &[String]) -> rusqlite::Result<usize> {
        let (clause, values) = in_clause("id", ids);
        self.with_conn(|c| {
            c.execute(
                &format!("DELETE FROM image_embeddings WHERE {clause}"),
                params_from_iter(values),
            )
        })
    }

    pub fn archived_conversation_list(&self) -> rusqlite::Result<Vec<ArchivedConversationRow>> {
        self.with_conn(|c| {
            let mut stmt =
                c.prepare("SELECT conversation_id, conversation_date FROM archived_conversations")?;
            stmt.query_map([], |r| {
                Ok(ArchivedConversationRow {
                    conversation_id: r.get(0)?,
                    conversation_date: r.get(1)?,
                })
            })?
            .collect()
        })
    }

    pub fn archived_conversation_save(
        &self,
        row: &ArchivedConversationRow,
    ) -> rusqlite::Result<()> {
        self.with_conn(|c| {
            c.execute(
                "INSERT OR REPLACE INTO archived_conversations (conversation_id, conversation_date)
                 VALUES (?1, ?2)",
                params![row.conversation_id, row.conversation_date],
            )?;
            Ok(())
        })
    }

    pub fn archived_conversation_delete(&self, conversation_id: &str) -> rusqlite::Result<usize> {
        self.with_conn(|c| {
            c.execute(
                "DELETE FROM archived_conversations WHERE conversation_id=?1",
                [conversation_id],
            )
        })
    }

    pub fn trashed_message_ids(&self) -> rusqlite::Result<Vec<String>> {
        self.with_conn(|c| {
            let mut stmt = c.prepare("SELECT message_id FROM trashed_sms")?;
            stmt.query_map([], |r| r.get(0))?.collect()
        })
    }

    pub fn trashed_message_save_many(&self, rows: &[TrashedMessageRow]) -> rusqlite::Result<usize> {
        self.with_conn(|c| {
            let tx = c.unchecked_transaction()?;
            let mut inserted = 0;
            {
                let mut stmt = tx.prepare(
                    "INSERT OR REPLACE INTO trashed_sms (message_id, is_mms, trashed_at)
                     VALUES (?1, ?2, ?3)",
                )?;
                for row in rows {
                    inserted +=
                        stmt.execute(params![row.message_id, row.is_mms as i64, row.trashed_at])?;
                }
            }
            tx.commit()?;
            Ok(inserted)
        })
    }

    pub fn trashed_message_delete_by_ids(&self, ids: &[String]) -> rusqlite::Result<usize> {
        let (clause, values) = in_clause("message_id", ids);
        self.with_conn(|c| {
            c.execute(
                &format!("DELETE FROM trashed_sms WHERE {clause}"),
                params_from_iter(values),
            )
        })
    }

    pub fn trashed_message_delete_older_than(&self, before: &str) -> rusqlite::Result<usize> {
        self.with_conn(|c| c.execute("DELETE FROM trashed_sms WHERE trashed_at < ?1", [before]))
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/library/db/simple.rs"]
mod simple_tests;
