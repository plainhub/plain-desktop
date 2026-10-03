use crate::{
    db::Db,
    enums::DataType,
    library::{LibraryError, LibraryResult, audio_queue},
};
use rusqlite::params;
#[derive(Clone)]
pub struct Item {
    pub media_type: i32,
    pub id: String,
    pub path: String,
}
pub fn cleanup(db: &Db, items: &[Item], paths: &[String], roots: &[String]) -> LibraryResult<()> {
    if items.iter().any(|item| {
        !matches!(item.media_type, 1 | 2 | 3 | 24)
            || item.id.is_empty()
            || !paths.contains(&item.path)
    }) {
        return Err(LibraryError::Other("invalid deleted media receipt".into()));
    }
    db.with_conn(|c| {
        let tx=c.unchecked_transaction()?;
        for item in items {
            tx.execute("DELETE FROM tag_relations WHERE type=?1 AND key=?2",params![item.media_type,item.id])?;
            match item.media_type {
                1|2=>{ tx.execute("DELETE FROM media_item WHERE media_type=?1 AND media_id=?2",params![if item.media_type==1 {"audio"}else{"video"},item.id])?;
                    if item.media_type==2 { tx.execute("DELETE FROM video_play_progress WHERE media_id=?1",[&item.id])?; }
                },
                3=>{tx.execute("DELETE FROM image_embeddings WHERE id=?1",[&item.id])?;}, _=>{},
            }
        }
        let encoded=serde_json::to_string(paths).map_err(|e|LibraryError::Other(e.to_string()))?;
        tx.execute("DELETE FROM tag_relations WHERE type=?1 AND key IN (SELECT value FROM json_each(?2))",params![DataType::File.kind(),encoded])?;
        tx.execute("DELETE FROM image_embeddings WHERE path IN (SELECT value FROM json_each(?1))",[&encoded])?;
        audio_queue::remove_paths_conn(&tx,paths)?;
        for root in roots {
            let prefix=format!("{}/",root.trim_end_matches('/'));
            tx.execute("DELETE FROM tag_relations WHERE type=?1 AND (key=?2 OR substr(key,1,length(?3))=?3)",params![DataType::File.kind(),root,prefix])?;
            tx.execute("DELETE FROM image_embeddings WHERE path=?1 OR substr(path,1,length(?2))=?2",params![root,prefix])?;
            audio_queue::remove_root_paths_conn(&tx,root)?;
        }
        tx.commit()?;Ok(())
    })
}
#[cfg(test)]
#[path = "../../tests/unit/library/media_deletes.rs"]
mod tests;
