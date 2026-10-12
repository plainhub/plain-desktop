use crate::db::Db;
use crate::enums::DataType;
use crate::error::DomainError;
use crate::error::DomainResult;
use crate::media::audio::queue as audio_queue;
use rusqlite::{OptionalExtension, Transaction, params};
use std::collections::HashSet;
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "graphql", derive(async_graphql::Enum))]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Action {
    Trash,
    Restore,
    Delete,
    Move,
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub path: String,
    pub destination_path: String,
}
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub successful: Vec<Item>,
    pub failed_ids: Vec<String>,
}
pub fn validate(
    kind: DataType,
    action: Action,
    requested: &[String],
    outcome: &Outcome,
) -> DomainResult<()> {
    if !matches!(
        kind,
        DataType::Audio | DataType::Video | DataType::Image | DataType::Doc
    ) || requested.len() > 500
    {
        return Err(invalid("invalid media action"));
    }
    let expected = requested.iter().collect::<HashSet<_>>();
    if expected.len() != requested.len() || requested.iter().any(|id| id.is_empty()) {
        return Err(invalid("invalid media selection"));
    }
    let mut actual = HashSet::new();
    for item in &outcome.successful {
        if !expected.contains(&item.id)
            || item.path.is_empty()
            || !actual.insert(&item.id)
            || (action == Action::Move && item.destination_path.is_empty())
        {
            return Err(invalid("invalid media action receipt"));
        }
    }
    for id in &outcome.failed_ids {
        if !expected.contains(id) || !actual.insert(id) {
            return Err(invalid("invalid media failure receipt"));
        }
    }
    if actual.len() != expected.len() {
        return Err(invalid("incomplete media action receipt"));
    }
    Ok(())
}
pub fn cleanup(
    db: &Db,
    kind: DataType,
    action: Action,
    successful: &[Item],
) -> DomainResult<usize> {
    if !matches!(
        kind,
        DataType::Audio | DataType::Video | DataType::Image | DataType::Doc | DataType::File
    ) {
        return Err(invalid("unsupported media kind"));
    }
    if successful.is_empty() {
        return Ok(successful.len());
    }
    let ids = serde_json::to_string(&successful.iter().map(|item| &item.id).collect::<Vec<_>>())
        .map_err(|e| invalid(&e.to_string()))?;
    let paths = successful
        .iter()
        .map(|item| item.path.clone())
        .collect::<Vec<_>>();
    db.with_conn(|c| {
        let tx=c.unchecked_transaction()?;
        for item in successful { update_catalog(&tx, kind, action, item)?; }
        if action!=Action::Move && action!=Action::Restore {
            tx.execute("DELETE FROM tag_relations WHERE type=?1 AND key IN (SELECT value FROM json_each(?2))",params![kind.kind(),ids])?;
            if matches!(kind,DataType::Audio|DataType::Video) {
                tx.execute("DELETE FROM media_item WHERE media_type=?1 AND media_id IN (SELECT value FROM json_each(?2))",params![if kind==DataType::Audio {"audio"}else{"video"},ids])?;
            }
            if kind==DataType::Video {
                tx.execute("DELETE FROM video_play_progress WHERE media_id IN (SELECT value FROM json_each(?1))",[&ids])?;
            }
        }
        if kind==DataType::Image && action!=Action::Restore {
            tx.execute("DELETE FROM image_embeddings WHERE id IN (SELECT value FROM json_each(?1))",[&ids])?;
        }
        if kind==DataType::Audio && action!=Action::Restore { audio_queue::remove_paths_conn(&tx,&paths)?; }
        tx.commit()?;
        Ok(successful.len())
    })
}
fn update_catalog(
    tx: &Transaction<'_>,
    kind: DataType,
    action: Action,
    item: &Item,
) -> rusqlite::Result<()> {
    let row: Option<(String, Option<String>, bool)> = tx
        .query_row(
            "SELECT path,original_path,is_trashed FROM media_files WHERE id=?1 AND data_type=?2",
            params![item.id, kind.kind()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((path, original_path, trashed)) = row else {
        return Ok(());
    };
    if path != item.path {
        return Err(rusqlite::Error::InvalidQuery);
    }
    if action == Action::Delete {
        tx.execute(
            "DELETE FROM media_files WHERE id=?1 AND data_type=?2 AND path=?3",
            params![item.id, kind.kind(), item.path],
        )?;
        return Ok(());
    }
    let (destination, next_original, next_trashed) = match action {
        Action::Trash if !trashed => (
            if item.destination_path.is_empty() {
                path.clone()
            } else {
                item.destination_path.clone()
            },
            Some(path),
            true,
        ),
        Action::Restore if trashed => (
            if item.destination_path.is_empty() {
                original_path.unwrap_or(path)
            } else {
                item.destination_path.clone()
            },
            None,
            false,
        ),
        Action::Move if !trashed && !item.destination_path.is_empty() => {
            (item.destination_path.clone(), None, false)
        }
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    let destination_path = std::path::Path::new(&destination);
    if !destination_path.is_absolute() {
        return Err(rusqlite::Error::InvalidQuery);
    }
    let parent = destination_path
        .parent()
        .and_then(|path| path.to_str())
        .ok_or(rusqlite::Error::InvalidQuery)?;
    let name = destination_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(rusqlite::Error::InvalidQuery)?;
    tx.execute(
        "UPDATE media_files SET path=?1,file_name=?2,mime_type=?3,bucket_id=?4,is_trashed=?5,original_path=?6 WHERE id=?7 AND data_type=?8 AND path=?9",
        params![destination,name,crate::utils::mime::mime_from_ext(&destination),parent,next_trashed,next_original,item.id,kind.kind(),item.path],
    )?;
    Ok(())
}

fn invalid(message: &str) -> DomainError {
    DomainError::Other(message.into())
}
#[cfg(test)]
#[path = "../../tests/unit/media/actions.rs"]
mod tests;
