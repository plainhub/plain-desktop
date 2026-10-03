use crate::{
    db::Db,
    enums::DataType,
    library::{LibraryError, LibraryResult, audio_queue},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    pub media_type: i32,
    pub source_id: String,
    pub destination_id: String,
    pub source_path: String,
    pub destination_path: String,
}
fn kind(value: i32) -> LibraryResult<DataType> {
    match value {
        1 => Ok(DataType::Audio),
        2 => Ok(DataType::Video),
        3 => Ok(DataType::Image),
        24 => Ok(DataType::Doc),
        _ => Err(invalid("unsupported media move type")),
    }
}
pub fn rebind(
    db: &Db,
    bindings: &[Binding],
    source_root: &str,
    destination_root: &str,
) -> LibraryResult<usize> {
    let mut sources = HashSet::new();
    let mut destinations = HashSet::new();
    for binding in bindings {
        kind(binding.media_type)?;
        if binding.source_id.is_empty()
            || binding.destination_id.is_empty()
            || binding.source_path.is_empty()
            || binding.destination_path.is_empty()
            || !sources.insert((binding.media_type, binding.source_id.as_str()))
            || !destinations.insert((binding.media_type, binding.destination_id.as_str()))
        {
            return Err(invalid("invalid media move binding"));
        }
    }
    if source_root.is_empty() || destination_root.is_empty() {
        return Err(invalid("file move roots required"));
    }
    db.with_conn(|c| {
        let tx = c.unchecked_transaction()?;
        let mut snapshots = Vec::new();
        for binding in bindings {
            let media_type = kind(binding.media_type)?;
            let mut statement = tx.prepare("SELECT tag_id,created_at,size,title FROM tag_relations WHERE type=?1 AND key=?2")?;
            let tags = statement.query_map(params![binding.media_type,binding.source_id],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,i64>(2)?,row.get::<_,String>(3)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
            let duration = if matches!(media_type,DataType::Audio|DataType::Video) {
                tx.query_row("SELECT duration_ms,updated_at FROM media_item WHERE media_type=?1 AND media_id=?2",params![media_type.media_type_str().unwrap(),binding.source_id],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?))).optional()?
            } else { None };
            let progress = if media_type==DataType::Video {
                tx.query_row("SELECT position_ms,updated_at FROM video_play_progress WHERE media_id=?1",[&binding.source_id],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?))).optional()?
            } else { None };
            snapshots.push((binding,media_type,tags,duration,progress));
        }
        for (binding,media_type,_,_,_) in &snapshots {
            if binding.source_id==binding.destination_id { continue; }
            tx.execute("DELETE FROM tag_relations WHERE type=?1 AND key=?2",params![binding.media_type,binding.source_id])?;
            if matches!(media_type,DataType::Audio|DataType::Video) {
                tx.execute("DELETE FROM media_item WHERE media_type=?1 AND media_id=?2",params![media_type.media_type_str().unwrap(),binding.source_id])?;
            }
            if *media_type==DataType::Video { tx.execute("DELETE FROM video_play_progress WHERE media_id=?1",[&binding.source_id])?; }
        }
        for (binding,media_type,tags,duration,progress) in snapshots {
            if binding.source_id!=binding.destination_id {
                for (tag,created,size,title) in tags {
                    tx.execute("INSERT INTO tag_relations(tag_id,key,type,created_at,size,title) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(tag_id,key,type) DO UPDATE SET created_at=min(tag_relations.created_at,excluded.created_at),size=excluded.size,title=excluded.title",params![tag,binding.destination_id,binding.media_type,created,size,title])?;
                }
                if matches!(media_type,DataType::Audio|DataType::Video) {
                    let name = media_type.media_type_str().unwrap();
                    tx.execute("DELETE FROM media_item WHERE media_type=?1 AND media_id=?2",params![name,binding.destination_id])?;
                    if let Some((duration,updated)) = duration {
                        tx.execute("INSERT INTO media_item(media_type,media_id,duration_ms,updated_at) VALUES(?1,?2,?3,?4)",params![name,binding.destination_id,duration,updated])?;
                    }
                }
                if media_type==DataType::Video {
                    tx.execute("DELETE FROM video_play_progress WHERE media_id=?1",[&binding.destination_id])?;
                    if let Some((position,updated)) = progress {
                        tx.execute("INSERT INTO video_play_progress(media_id,position_ms,updated_at) VALUES(?1,?2,?3)",params![binding.destination_id,position,updated])?;
                    }
                }
            }
            if media_type==DataType::Image { tx.execute("DELETE FROM image_embeddings WHERE id IN (?1,?2)",params![binding.source_id,binding.destination_id])?; }
            if media_type==DataType::Audio { audio_queue::remove_paths_conn(&tx,std::slice::from_ref(&binding.source_path))?; }
        }
        audio_queue::remove_root_paths_conn(&tx, source_root)?;
        let source_prefix = format!("{}/",source_root.trim_end_matches('/'));
        let mut statement = tx.prepare("SELECT tag_id,key,created_at,size,title FROM tag_relations WHERE type=?1 AND (key=?2 OR substr(key,1,length(?3))=?3)")?;
        let relations = statement.query_map(params![DataType::File.kind(),source_root,source_prefix],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,i64>(3)?,row.get::<_,String>(4)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);
        for (tag,key,created,size,title) in relations {
            let relative = if key==source_root { "" } else { &key[source_prefix.len()..] };
            let destination = if relative.is_empty() { destination_root.to_owned() } else { format!("{}/{relative}",destination_root.trim_end_matches('/')) };
            if destination==key { continue; }
            tx.execute("INSERT INTO tag_relations(tag_id,key,type,created_at,size,title) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(tag_id,key,type) DO UPDATE SET created_at=min(tag_relations.created_at,excluded.created_at),size=excluded.size,title=excluded.title",params![tag,destination,DataType::File.kind(),created,size,title])?;
            tx.execute("DELETE FROM tag_relations WHERE tag_id=?1 AND key=?2 AND type=?3",params![tag,key,DataType::File.kind()])?;
        }
        tx.commit()?;
        Ok(bindings.len())
    })
}
fn invalid(message: &str) -> LibraryError {
    LibraryError::Other(message.into())
}
#[cfg(test)]
#[path = "../../tests/unit/library/media_moves.rs"]
mod tests;
