use crate::db::Db;
use anyhow::{Result, bail};
use rusqlite::{OptionalExtension, params};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

pub enum Selection<'a> {
    Ids(&'a [String]),
    Peer(&'a str),
    Channel(&'a str),
}

pub fn delete(db: &Db, directory: &Path, selection: Selection<'_>) -> Result<usize> {
    let _guard = db.app_files_lock()?;
    let mut staged: Vec<(PathBuf, PathBuf)> = Vec::new();
    let result = db.with_conn(|connection| -> Result<usize> {
        let tx = connection.unchecked_transaction()?;
        let rows: Vec<(String, String)> = match selection {
            Selection::Ids(ids) => {
                let mut statement = tx.prepare("SELECT id,content FROM chats WHERE id=?1")?;
                let mut rows = Vec::new();
                for id in ids.iter().collect::<BTreeSet<_>>() {
                    if let Some(row) = statement
                        .query_row([id], |r| Ok((r.get(0)?, r.get(1)?)))
                        .optional()?
                    {
                        rows.push(row);
                    }
                }
                rows
            }
            Selection::Peer(id) => {
                if id.is_empty() {
                    bail!("peer id is empty");
                }
                let mut statement = tx.prepare(
                    "SELECT id,content FROM chats WHERE channel_id='' AND (from_id=?1 OR to_id=?1)",
                )?;
                statement
                    .query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<rusqlite::Result<_>>()?
            }
            Selection::Channel(id) => {
                if id.is_empty() {
                    bail!("channel id is empty");
                }
                let mut statement =
                    tx.prepare("SELECT id,content FROM chats WHERE channel_id=?1")?;
                statement
                    .query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<rusqlite::Result<_>>()?
            }
        };
        let mut releases: BTreeMap<String, i64> = BTreeMap::new();
        for (_, content) in &rows {
            let content: serde_json::Value = serde_json::from_str(content)?;
            if !matches!(content["type"].as_str(), Some("IMAGES" | "FILES")) {
                continue;
            }
            if let Some(items) = content["value"]["items"].as_array() {
                for item in items {
                    if let Some(suffix) = item["uri"]
                        .as_str()
                        .and_then(|uri| uri.strip_prefix("fid:"))
                    {
                        let id = suffix.split('.').next().unwrap_or_default();
                        if !id.is_empty() {
                            *releases.entry(id.to_owned()).or_default() += 1;
                        }
                    }
                }
            }
        }
        for (id, count) in releases {
            let file: Option<(String, i64)> = tx
                .query_row(
                    "SELECT real_path,ref_count FROM app_files WHERE id=?1",
                    [&id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let Some((relative, refs)) = file else {
                continue;
            };
            if refs > count {
                tx.execute(
                    "UPDATE app_files SET ref_count=ref_count-?1,updated_at=?2 WHERE id=?3",
                    params![count, crate::db::now_iso(), id],
                )?;
            } else {
                let path = super::owned_path(directory, &relative)?;
                match fs::symlink_metadata(&path) {
                    Ok(metadata) => {
                        if !metadata.is_file() {
                            bail!("attachment is not a regular file");
                        }
                        let quarantine = path.with_file_name(format!(
                            ".release-{}",
                            crate::utils::short_uuid::short_uuid()
                        ));
                        fs::rename(&path, &quarantine)?;
                        staged.push((path, quarantine));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
                tx.execute("DELETE FROM app_files WHERE id=?1", [&id])?;
            }
        }
        for (id, _) in &rows {
            tx.execute("DELETE FROM chats WHERE id=?1", [id])?;
        }
        tx.commit()?;
        Ok(rows.len())
    });
    match result {
        Ok(count) => {
            let mut failures = Vec::new();
            for (_, quarantine) in staged {
                if let Err(error) = fs::remove_file(&quarantine) {
                    failures.push(format!("{}: {error}", quarantine.display()));
                }
            }
            if !failures.is_empty() {
                bail!("attachment cleanup failed: {}", failures.join("; "));
            }
            Ok(count)
        }
        Err(error) => {
            let mut failures = Vec::new();
            for (path, quarantine) in staged.into_iter().rev() {
                if let Err(restore) = fs::rename(&quarantine, &path) {
                    failures.push(format!("{}: {restore}", path.display()));
                }
            }
            if !failures.is_empty() {
                bail!(
                    "{error}; attachment restore failed: {}",
                    failures.join("; ")
                );
            }
            Err(error)
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/chat/app_file_store/chat_deletion.rs"]
mod tests;
