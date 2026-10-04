use crate::db::Db;
use anyhow::{Result, bail};
use rusqlite::{OptionalExtension, params};
#[cfg(test)]
use std::fs;
use std::{
    collections::{BTreeMap, BTreeSet},
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
    Ok(mutate(db, directory, selection, None)?.count)
}

pub fn remove_channel(db: &Db, directory: &Path, id: &str) -> Result<Option<crate::db::DChannel>> {
    Ok(mutate(db, directory, Selection::ChannelRecord(id), None)?.channel)
}

pub fn remove_channel_if_matches(
    db: &Db,
    directory: &Path,
    expected: &crate::db::DChannel,
) -> Result<Option<crate::db::DChannel>> {
    Ok(mutate(
        db,
        directory,
        Selection::ChannelRecord(&expected.id),
        Some(expected),
    )?
    .channel)
}

struct Outcome {
    count: usize,
    channel: Option<crate::db::DChannel>,
}

fn mutate(
    db: &Db,
    directory: &Path,
    selection: Selection<'_>,
    expected: Option<&crate::db::DChannel>,
) -> Result<Outcome> {
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
        if expected.is_some_and(|row| channel.as_ref() != Some(row)) {
            bail!("Channel changed during mutation");
        }
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
            for (id, count) in super::content_refs::collect(&content) {
                *releases.entry(id).or_default() += count;
            }
        }
        super::content_refs::release(&tx, directory, releases, &mut staged)?;
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
                    tx.execute("DELETE FROM chat_receipts WHERE peer_id=?1", [id])?;
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
    super::content_refs::finish(result, staged)
}

#[cfg(test)]
#[path = "../../../tests/unit/chat/app_file_store/chat_deletion.rs"]
mod tests;
