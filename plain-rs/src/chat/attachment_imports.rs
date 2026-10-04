use super::app_file_store::{self, ImportResult};
use crate::db::{Db, chat_store::messages as chats};
use anyhow::{Result, bail};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ticket {
    pub token: String,
    pub path: String,
    pub file_name: String,
    pub size: i64,
}
struct Pending {
    ticket: Ticket,
    message_id: String,
    id: String,
    uri: String,
}
#[derive(Default)]
pub struct Imports(Mutex<HashMap<String, Pending>>);
impl Imports {
    pub fn begin(
        &self,
        db: &Db,
        directory: &Path,
        message_id: &str,
        id: &str,
        uri: &str,
    ) -> Result<Ticket> {
        let mut pending = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Attachment lock unavailable"))?;
        if pending.len() >= 128 {
            bail!("Too many attachment transfers");
        }
        let chat =
            chats::get(db, message_id)?.ok_or_else(|| anyhow::anyhow!("Message unavailable"))?;
        let content: Value = serde_json::from_str(&chat.content)?;
        if !matches!(content["type"].as_str(), Some("FILES" | "IMAGES"))
            || !uri.starts_with("fsid:")
        {
            bail!("Message has no remote attachment");
        }
        let item = content["value"]["items"]
            .as_array()
            .and_then(|items| {
                items.iter().find(|item| {
                    item["id"].as_str() == Some(id) && item["uri"].as_str() == Some(uri)
                })
            })
            .ok_or_else(|| anyhow::anyhow!("Attachment changed or unavailable"))?;
        let size = item["size"]
            .as_i64()
            .filter(|n| *n >= 0)
            .ok_or_else(|| anyhow::anyhow!("Invalid attachment size"))?;
        let file_name = item["fileName"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid attachment name"))?
            .to_owned();
        let root = temp_root(directory)?;
        let token = uuid::Uuid::new_v4().to_string();
        let path = root.join(&token);
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let ticket = Ticket {
            token: token.clone(),
            path: path.to_string_lossy().into_owned(),
            file_name,
            size,
        };
        pending.insert(
            token,
            Pending {
                ticket: ticket.clone(),
                message_id: message_id.into(),
                id: id.into(),
                uri: uri.into(),
            },
        );
        Ok(ticket)
    }
    pub fn finish(&self, db: &Db, directory: &Path, token: &str) -> Result<ImportResult> {
        let mut pending = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Attachment lock unavailable"))?;
        let transfer = pending
            .remove(token)
            .ok_or_else(|| anyhow::anyhow!("Attachment ticket unavailable"))?;
        let path = Path::new(&transfer.ticket.path);
        let result = (|| {
            let root = temp_root(directory)?;
            let metadata = fs::symlink_metadata(path)?;
            if !metadata.is_file()
                || metadata.len() != transfer.ticket.size as u64
                || path.canonicalize()?.parent() != Some(root.as_path())
            {
                bail!("Incomplete or invalid attachment download");
            }
            Ok(app_file_store::import_attachment(
                db,
                directory,
                path,
                &transfer.ticket.file_name,
                &transfer.message_id,
                &transfer.id,
                &transfer.uri,
            )?)
        })();
        let _ = fs::remove_file(path);
        result
    }
    pub fn abort(&self, token: &str) -> Result<bool> {
        let transfer = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("Attachment lock unavailable"))?
            .remove(token);
        if let Some(transfer) = transfer {
            let _ = fs::remove_file(transfer.ticket.path);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
impl Drop for Imports {
    fn drop(&mut self) {
        if let Ok(pending) = self.0.get_mut() {
            for transfer in pending.values() {
                let _ = fs::remove_file(&transfer.ticket.path);
            }
        }
    }
}
fn temp_root(directory: &Path) -> Result<PathBuf> {
    let directory = directory.canonicalize()?;
    let root = directory.join("attachment-transfers");
    fs::create_dir_all(&root)?;
    if fs::symlink_metadata(&root)?.file_type().is_symlink()
        || root.canonicalize()?.parent() != Some(directory.as_path())
    {
        bail!("Invalid attachment directory");
    }
    Ok(root)
}
#[cfg(test)]
#[path = "../../tests/unit/chat/attachment_imports.rs"]
mod tests;
