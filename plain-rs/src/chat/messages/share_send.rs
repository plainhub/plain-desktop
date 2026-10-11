use crate::chat::attachments::app_file_store;
use crate::db::Db;
use anyhow::Result;
use serde_json::{Value, json};
use std::path::Path;
pub fn text(db: &Db, directory: &Path, text: &str) -> Result<Value> {
    if text.encode_utf16().count() <= 2048 {
        return Ok(json!({"type":"TEXT","value":{"text":text}}));
    }
    let imported = app_file_store::import_bytes(db, directory, text.as_bytes(), "text/plain")?;
    let summary: String = text
        .chars()
        .scan(0usize, |units, c| {
            *units += c.len_utf16();
            (*units <= 250).then_some(c)
        })
        .collect();
    Ok(
        json!({"type":"FILES","value":{"items":[{"id":crate::utils::short_uuid::short_uuid(),"uri":format!("fid:{}",imported.fid_suffix),"fileName":format!("message-{}.txt",chrono::Utc::now().timestamp_millis()),"size":text.len(),"summary":summary,"width":0,"height":0,"durationMs":0}]}}),
    )
}
