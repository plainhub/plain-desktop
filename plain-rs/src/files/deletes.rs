use crate::db::Db;
use crate::enums::DataType;
use crate::error::DomainError;
use crate::error::DomainResult;
use crate::media::audio::queue as audio_queue;
use rusqlite::params;
#[derive(Clone)]
pub struct Item {
    pub media_type: i32,
    pub id: String,
    pub path: String,
}
pub fn cleanup(db: &Db, items: &[Item], paths: &[String], roots: &[String]) -> DomainResult<()> {
    if items.iter().any(|item| {
        !matches!(item.media_type, 1 | 2 | 3 | 24)
            || item.id.is_empty()
            || !paths.contains(&item.path)
    }) {
        return Err(DomainError::Other("invalid deleted media receipt".into()));
    }
    db.with_conn(|c| {
        let tx=c.unchecked_transaction()?;
        let encoded=serde_json::to_string(paths).map_err(|e|DomainError::Other(e.to_string()))?;
        let mut media = items.to_vec();
        {
            let mut statement=tx.prepare("SELECT data_type,id,path FROM media_files WHERE path IN (SELECT value FROM json_each(?1))")?;
            media.extend(statement.query_map([&encoded],|row|Ok(Item { media_type:row.get(0)?,id:row.get(1)?,path:row.get(2)? }))?.collect::<rusqlite::Result<Vec<_>>>()?);
        }
        for root in roots {
            let prefix=directory_prefix(root);
            let mut statement=tx.prepare("SELECT data_type,id,path FROM media_files WHERE path=?1 OR substr(path,1,length(?2))=?2")?;
            media.extend(statement.query_map(params![root,prefix],|row|Ok(Item { media_type:row.get(0)?,id:row.get(1)?,path:row.get(2)? }))?.collect::<rusqlite::Result<Vec<_>>>()?);
        }
        let mut seen=std::collections::HashSet::new();
        for item in &media {
            if !seen.insert((item.media_type,&item.id)) { continue; }
            tx.execute("DELETE FROM tag_relations WHERE type=?1 AND key=?2",params![item.media_type,item.id])?;
            match item.media_type {
                1|2=>{ tx.execute("DELETE FROM media_item WHERE media_type=?1 AND media_id=?2",params![if item.media_type==1 {"audio"}else{"video"},item.id])?;
                    if item.media_type==2 { tx.execute("DELETE FROM video_play_progress WHERE media_id=?1",[&item.id])?; }
                },
                3=>{tx.execute("DELETE FROM image_embeddings WHERE id=?1",[&item.id])?;}, _=>{},
            }
        }
        tx.execute("DELETE FROM media_files WHERE path IN (SELECT value FROM json_each(?1))",[&encoded])?;
        tx.execute("DELETE FROM tag_relations WHERE type=?1 AND key IN (SELECT value FROM json_each(?2))",params![DataType::File.kind(),encoded])?;
        tx.execute("DELETE FROM image_embeddings WHERE path IN (SELECT value FROM json_each(?1))",[&encoded])?;
        audio_queue::remove_paths_conn(&tx,paths)?;
        for root in roots {
            let prefix=directory_prefix(root);
            tx.execute("DELETE FROM media_files WHERE path=?1 OR substr(path,1,length(?2))=?2",params![root,prefix])?;
            tx.execute("DELETE FROM tag_relations WHERE type=?1 AND (key=?2 OR substr(key,1,length(?3))=?3)",params![DataType::File.kind(),root,prefix])?;
            tx.execute("DELETE FROM image_embeddings WHERE path=?1 OR substr(path,1,length(?2))=?2",params![root,prefix])?;
            audio_queue::remove_root_paths_conn(&tx,root)?;
        }
        tx.commit()?;Ok(())
    })
}
fn directory_prefix(root: &str) -> String {
    let separator = if cfg!(windows) && !root.contains('/') {
        '\\'
    } else {
        '/'
    };
    format!("{}{separator}", root.trim_end_matches(separator))
}

#[cfg(test)]
#[path = "../../tests/unit/files/deletes.rs"]
mod tests;
