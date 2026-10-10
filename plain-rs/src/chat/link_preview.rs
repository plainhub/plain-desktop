use regex::Regex;
use std::collections::HashSet;
use std::sync::OnceLock;
fn url_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)https?://(?:[-\w.])+(?:\:[0-9]+)?(?:/(?:[\w/_.-]*(?:\?[\w&=%.+-]*)?(?:#[\w.-]*)?)?)?")
            .expect("url regex")
    })
}

/// Extract up to 5 distinct valid URLs from `text`. Mirrors
/// `LinkPreviewHelper.extractUrls`.
pub fn extract_urls(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut urls = Vec::new();
    for m in url_regex().find_iter(text) {
        let url = m.as_str();
        if is_valid_url(url) && seen.insert(url.to_string()) {
            urls.push(url.to_string());
            if urls.len() == 5 {
                break;
            }
        }
    }
    urls
}

pub fn is_valid_url(url: &str) -> bool {
    let without_protocol = url.split_once("://").map(|(_, r)| r).unwrap_or("");
    if without_protocol.is_empty() {
        return false;
    }
    let host = without_protocol
        .split(['/', ':'])
        .next()
        .unwrap_or("")
        .to_lowercase();
    if host.is_empty() {
        return false;
    }
    if host == "localhost"
        || host.starts_with("127.")
        || host.starts_with("192.168.")
        || host.starts_with("10.")
    {
        return false;
    }
    // 172.16.0.0/12 .. 172.31.255.255
    if host.starts_with("172.") {
        let octet: u16 = host
            .split('.')
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        if (16..=31).contains(&octet) {
            return false;
        }
    }
    true
}

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
        let urls = extract_urls(text);
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
pub fn append(db: &Db, id: &str, text: &str, preview: &Value) -> Result<Option<DChat>> {
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
#[path = "../../tests/unit/chat/link_preview/commit.rs"]
mod tests;
