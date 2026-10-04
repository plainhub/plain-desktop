use crate::chat::app_file_store::content_refs;
use crate::db::{CHAT_COLS, DChat, Db, now_iso, row_to_chat};
use anyhow::{Result, bail};
use rusqlite::{OptionalExtension, params};
use serde::Serialize;
use serde_json::Value;
use std::path::Path;
#[derive(Serialize)]
pub struct Edit {
    pub changed: bool,
    pub chat: DChat,
}
pub fn edit(db: &Db, directory: &Path, id: &str, text: &str) -> Result<Edit> {
    let _files = db.app_files_lock()?;
    let mut staged = Vec::new();
    let result = db.with_conn(|conn| -> Result<Edit> {
        let tx = conn.unchecked_transaction()?;
        let mut row = tx
            .query_row(
                &format!("SELECT {CHAT_COLS} FROM chats WHERE id=?1"),
                [id],
                row_to_chat,
            )
            .optional()?
            .ok_or_else(|| anyhow::anyhow!("Message unavailable"))?;
        let old: Value = serde_json::from_str(&row.content)?;
        if old["type"] != "TEXT" {
            bail!("Message is not text");
        }
        let urls = super::extract_urls(text);
        let mut content = old.clone();
        content["value"]["text"] = Value::String(text.into());
        let previews = old["value"]["linkPreviews"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|preview| {
                preview["url"]
                    .as_str()
                    .is_some_and(|url| urls.iter().any(|candidate| candidate == url))
            })
            .collect::<Vec<_>>();
        if !previews.is_empty() || old["value"].get("linkPreviews").is_some() {
            content["value"]["linkPreviews"] = Value::Array(previews);
        }
        if content == old {
            return Ok(Edit {
                changed: false,
                chat: row,
            });
        }
        let remaining = content_refs::collect(&content);
        let removed = content_refs::collect(&old)
            .into_iter()
            .filter_map(|(id, count)| {
                let delta = count - remaining.get(&id).copied().unwrap_or(0);
                (delta > 0).then_some((id, delta))
            })
            .collect();
        content_refs::release(&tx, directory, removed, &mut staged)?;
        row.content = content.to_string();
        row.updated_at = now_iso();
        if tx.execute(
            "UPDATE chats SET content=?2,updated_at=?3 WHERE id=?1",
            params![row.id, row.content, row.updated_at],
        )? != 1
        {
            bail!("Message update failed");
        }
        tx.commit()?;
        Ok(Edit {
            changed: true,
            chat: row,
        })
    });
    content_refs::finish(result, staged)
}
pub(super) fn append(db: &Db, id: &str, text: &str, preview: &Value) -> Result<Option<DChat>> {
    db.with_conn(|conn| -> Result<Option<DChat>> {
        let tx = conn.unchecked_transaction()?;
        let Some(mut row) = tx
            .query_row(
                &format!("SELECT {CHAT_COLS} FROM chats WHERE id=?1"),
                [id],
                row_to_chat,
            )
            .optional()?
        else {
            return Ok(None);
        };
        let mut content: Value = serde_json::from_str(&row.content)?;
        if content["type"] != "TEXT" || content["value"]["text"].as_str() != Some(text) {
            return Ok(None);
        }
        let mut previews = content["value"]["linkPreviews"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if previews.iter().any(|p| p["url"] == preview["url"]) {
            return Ok(None);
        }
        previews.push(preview.clone());
        content["value"]["linkPreviews"] = Value::Array(previews);
        row.content = content.to_string();
        row.updated_at = now_iso();
        if tx.execute(
            "UPDATE chats SET content=?2,updated_at=?3 WHERE id=?1",
            params![row.id, row.content, row.updated_at],
        )? != 1
        {
            bail!("Message update failed");
        }
        tx.commit()?;
        Ok(Some(row))
    })
}

#[cfg(test)]
#[path = "../../tests/unit/link_preview/commit.rs"]
mod tests;
