use super::{app_file_store::content_refs, enums::ChatStatus};
use crate::db::{CHAT_COLS, DChat, Db, now_iso, row_to_chat};
use anyhow::{Result, ensure};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
use std::{collections::HashSet, path::Path};
pub fn target(encoded: &str) -> Result<(String, String)> {
    if let Some(id) = encoded.strip_prefix("channel:") {
        ensure!(!id.is_empty(), "Missing channel target");
        Ok((String::new(), id.into()))
    } else {
        let id = encoded.strip_prefix("peer:").unwrap_or(encoded);
        ensure!(!id.is_empty(), "Missing peer target");
        Ok((id.into(), String::new()))
    }
}
fn files(items: &[Value]) -> Result<()> {
    let mut ids = HashSet::new();
    for item in items {
        let id = item["id"]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Missing attachment ID"))?;
        ensure!(ids.insert(id), "Duplicate attachment ID");
        ensure!(
            item.is_object()
                && item["uri"].is_string()
                && item["size"].as_i64().is_some_and(|n| n >= 0),
            "Invalid attachment facts"
        );
    }
    Ok(())
}
pub fn create_files(db: &Db, encoded: &str, items: Vec<Value>, images: bool) -> Result<DChat> {
    files(&items)?;
    let (to, channel) = target(encoded)?;
    super::message_lifecycle::create(
        db,
        &to,
        &channel,
        &json!({"type":if images{"IMAGES"}else{"FILES"},"value":{"items":items}}).to_string(),
    )
}
pub fn replace_files(
    db: &Db,
    directory: &Path,
    id: &str,
    items: Vec<Value>,
) -> Result<Option<DChat>> {
    Ok(replace_many(db, directory, &[id.to_owned()], items)?
        .into_iter()
        .next()
        .flatten())
}
pub fn replace_many(
    db: &Db,
    directory: &Path,
    ids: &[String],
    items: Vec<Value>,
) -> Result<Vec<Option<DChat>>> {
    files(&items)?;
    let unique: HashSet<_> = ids.iter().collect();
    ensure!(
        unique.len() == ids.len() && ids.len() <= 128,
        "Invalid attachment message selection"
    );
    let _files = db.app_files_lock()?;
    let mut staged = vec![];
    let result = db.with_conn(|c| -> Result<Vec<Option<DChat>>> {
        let tx = c.unchecked_transaction()?;
        let mut rows = Vec::with_capacity(ids.len());
        for id in ids {
            rows.push(replace(&tx, directory, id, &items, &mut staged)?);
        }
        tx.commit()?;
        Ok(rows)
    });
    content_refs::finish(result, staged)
}
fn replace(
    tx: &rusqlite::Transaction<'_>,
    directory: &Path,
    id: &str,
    items: &[Value],
    staged: &mut Vec<(std::path::PathBuf, std::path::PathBuf)>,
) -> Result<Option<DChat>> {
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
    ensure!(
        row.from_id == "me",
        "Only outgoing attachments can be replaced"
    );
    let old: Value = serde_json::from_str(&row.content)?;
    ensure!(
        matches!(old["type"].as_str(), Some("FILES" | "IMAGES")),
        "Message has no attachments"
    );
    let mut content = old.clone();
    let mut items = items.to_vec();
    for item in &mut items {
        if let Some(suffix) = item["uri"].as_str().and_then(|v| v.strip_prefix("fid:")) {
            let file_id = suffix.split('.').next().unwrap_or_default();
            let size: i64 =
                tx.query_row("SELECT size FROM app_files WHERE id=?1", [file_id], |r| {
                    r.get(0)
                })?;
            item["size"] = json!(size);
        }
    }
    content["value"]["items"] = Value::Array(items);
    let before = content_refs::collect(&old);
    let after = content_refs::collect(&content);
    let added = after
        .iter()
        .filter_map(|(id, n)| {
            let delta = n - before.get(id).copied().unwrap_or(0);
            (delta > 0).then_some((id.clone(), delta))
        })
        .collect();
    let removed = before
        .into_iter()
        .filter_map(|(id, n)| {
            let delta = n - after.get(&id).copied().unwrap_or(0);
            (delta > 0).then_some((id, delta))
        })
        .collect();
    content_refs::claim(tx, added)?;
    content_refs::release(tx, directory, removed, staged)?;
    row.content = content.to_string();
    row.status = if row.channel_id.is_empty() && (row.to_id.is_empty() || row.to_id == "local") {
        ChatStatus::Sent
    } else {
        ChatStatus::Pending
    };
    row.status_data.clear();
    row.updated_at = now_iso();
    tx.execute(
        "UPDATE chats SET content=?2,status=?3,status_data='',updated_at=?4 WHERE id=?1",
        params![row.id, row.content, row.status, row.updated_at],
    )?;
    Ok(Some(row))
}
#[cfg(test)]
#[path = "../../tests/unit/chat/message_commands.rs"]
mod tests;
