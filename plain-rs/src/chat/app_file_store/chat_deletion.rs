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
    PeerRecord(&'a str),
    ChannelRecord(&'a str),
}

pub fn delete(db: &Db, directory: &Path, selection: Selection<'_>) -> Result<usize> {
    Ok(mutate(db, directory, selection)?.count)
}

pub fn remove_channel(db: &Db, directory: &Path, id: &str) -> Result<Option<crate::db::DChannel>> {
    Ok(mutate(db, directory, Selection::ChannelRecord(id))?.channel)
}

struct Outcome {
    count: usize,
    channel: Option<crate::db::DChannel>,
}

fn mutate(db: &Db, directory: &Path, selection: Selection<'_>) -> Result<Outcome> {
    let _guard = db.app_files_lock()?;
    let mut staged: Vec<(PathBuf, PathBuf)> = Vec::new();
    let result = db.with_conn(|connection| -> Result<Outcome> {
        let tx = connection.unchecked_transaction()?;
        let channel = if let Selection::ChannelRecord(id) = &selection {
            let row = tx
                .query_row(
                    &format!(
                        "SELECT {} FROM chat_channels WHERE id=?1",
                        crate::db::CHANNEL_COLS
                    ),
                    [id],
                    crate::db::row_to_channel,
                )
                .optional()?;
            if row.is_none() {
                return Ok(Outcome {
                    count: 0,
                    channel: None,
                });
            }
            row
        } else {
            None
        };
        if let Selection::PeerRecord(id) = &selection {
            if !tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM peers WHERE id=?1)",
                [id],
                |r| r.get::<_, bool>(0),
            )? {
                return Ok(Outcome {
                    count: 0,
                    channel: None,
                });
            }
        }
        let rows: Vec<(String, String)> = match &selection {
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
            Selection::Peer(id) | Selection::PeerRecord(id) => {
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
            Selection::Channel(id) | Selection::ChannelRecord(id) => {
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
        let count = match selection {
            Selection::PeerRecord(id) => {
                let mut statement = tx.prepare("SELECT members FROM chat_channels")?;
                let members = statement
                    .query_map([], |r| r.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                let mut retained = false;
                for raw in members {
                    let members: Vec<crate::chat::channel::messages::ChannelMember> =
                        serde_json::from_str(&raw)?;
                    retained |= members.iter().any(|member| member.peer_id == id);
                }
                if retained {
                    tx.execute(
                        "UPDATE peers SET key='',status='CHANNEL',updated_at=?2 WHERE id=?1",
                        params![id, crate::db::now_iso()],
                    )?
                } else {
                    tx.execute("DELETE FROM peers WHERE id=?1", [id])?
                }
            }
            Selection::ChannelRecord(id) => {
                tx.execute("DELETE FROM chat_channels WHERE id=?1", [id])?
            }
            _ => rows.len(),
        };
        tx.commit()?;
        Ok(Outcome { count, channel })
    });
    match result {
        Ok(outcome) => {
            let mut failures = Vec::new();
            for (_, quarantine) in staged {
                if let Err(error) = fs::remove_file(&quarantine) {
                    failures.push(format!("{}: {error}", quarantine.display()));
                }
            }
            if !failures.is_empty() {
                bail!("attachment cleanup failed: {}", failures.join("; "));
            }
            Ok(outcome)
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
