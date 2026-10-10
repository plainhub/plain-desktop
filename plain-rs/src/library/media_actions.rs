use crate::{
    db::Db,
    enums::DataType,
    library::{LibraryError, LibraryResult, audio_queue},
};
use rusqlite::params;
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
) -> LibraryResult<()> {
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
) -> LibraryResult<usize> {
    if !matches!(
        kind,
        DataType::Audio | DataType::Video | DataType::Image | DataType::Doc | DataType::File
    ) {
        return Err(invalid("unsupported media kind"));
    }
    if action == Action::Restore || successful.is_empty() {
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
        if action!=Action::Move {
            tx.execute("DELETE FROM tag_relations WHERE type=?1 AND key IN (SELECT value FROM json_each(?2))",params![kind.kind(),ids])?;
            if matches!(kind,DataType::Audio|DataType::Video) {
                tx.execute("DELETE FROM media_item WHERE media_type=?1 AND media_id IN (SELECT value FROM json_each(?2))",params![if kind==DataType::Audio {"audio"}else{"video"},ids])?;
            }
            if kind==DataType::Video {
                tx.execute("DELETE FROM video_play_progress WHERE media_id IN (SELECT value FROM json_each(?1))",[&ids])?;
            }
        }
        if kind==DataType::Image {
            tx.execute("DELETE FROM image_embeddings WHERE id IN (SELECT value FROM json_each(?1))",[&ids])?;
        }
        if kind==DataType::Audio { audio_queue::remove_paths_conn(&tx,&paths)?; }
        tx.commit()?;
        Ok(successful.len())
    })
}
fn invalid(message: &str) -> LibraryError {
    LibraryError::Other(message.into())
}
#[cfg(test)]
#[path = "../../tests/unit/library/media_actions.rs"]
mod tests;
