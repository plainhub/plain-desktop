//! Row IO for the audio tables and settings. The playback-order state machine
//! Row IO for the audio queue tables. The playback-order state machine
//! lives in [`crate::library::audio_queue`].

use crate::db::Db;
pub use crate::db::models::audio_queue::{
    HISTORY_KEEP, PlayHistory, Playlist, PlaylistItem, QueueItem, QueueSource, QueueSourceKind,
};
use rusqlite::{Connection, OptionalExtension, params};

pub(crate) mod io {
    use super::*;
    pub fn get_source(c: &Connection) -> rusqlite::Result<QueueSource> {
        Ok(c.query_row("SELECT source,playlist_id,current_path,current_index,sort_by FROM audio_queue_source WHERE id=1", [], |r| Ok(QueueSource {
            source: QueueSourceKind::parse_stored(&r.get::<_,String>(0)?), playlist_id:r.get(1)?, current_path:r.get(2)?, current_index:r.get(3)?, sort_by:r.get(4)?
        })).optional()?.unwrap_or_default())
    }
    pub fn save_source(c: &Connection, src: &QueueSource) -> rusqlite::Result<()> {
        c.execute("INSERT INTO audio_queue_source (id,source,playlist_id,current_path,current_index,sort_by) VALUES (1,?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET source=?1,playlist_id=?2,current_path=?3,current_index=?4,sort_by=?5", params![src.source.as_str(),src.playlist_id,src.current_path,src.current_index,src.sort_by])?;
        Ok(())
    }
    fn queue_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<QueueItem> {
        Ok(QueueItem {
            path: r.get(0)?,
            sort_order: r.get(1)?,
            title: r.get(2)?,
            artist: r.get(3)?,
            duration_ms: r.get(4)?,
        })
    }
    pub fn all_queue_items(c: &Connection) -> rusqlite::Result<Vec<QueueItem>> {
        c.prepare("SELECT path,sort_order,title,artist,duration_ms FROM audio_queue_items ORDER BY sort_order ASC")?.query_map([],queue_row)?.collect()
    }
    pub fn queue_item_by_path(c: &Connection, path: &str) -> rusqlite::Result<Option<QueueItem>> {
        c.query_row(
            "SELECT path,sort_order,title,artist,duration_ms FROM audio_queue_items WHERE path=?",
            [path],
            queue_row,
        )
        .optional()
    }
    pub fn replace_queue_items(c: &Connection, items: &[QueueItem]) -> rusqlite::Result<()> {
        c.execute("DELETE FROM audio_queue_items", [])?;
        let mut insert=c.prepare("INSERT INTO audio_queue_items (path,sort_order,title,artist,duration_ms) VALUES (?1,?2,?3,?4,?5)")?;
        for (i, item) in items.iter().enumerate() {
            insert.execute(params![
                item.path,
                i as i64,
                item.title,
                item.artist,
                item.duration_ms
            ])?;
        }
        Ok(())
    }
    pub fn remove_queue_item(c: &Connection, path: &str) -> rusqlite::Result<()> {
        c.execute("DELETE FROM audio_queue_items WHERE path=?", [path])?;
        Ok(())
    }
    fn playlist_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Playlist> {
        Ok(Playlist {
            id: r.get(0)?,
            name: r.get(1)?,
            created_at: r.get(2)?,
            updated_at: r.get(3)?,
        })
    }
    pub fn all_playlists(c: &Connection) -> rusqlite::Result<Vec<Playlist>> {
        c.prepare("SELECT id,name,created_at,updated_at FROM audio_playlists ORDER BY updated_at DESC,rowid DESC")?.query_map([],playlist_row)?.collect()
    }
    pub fn playlist_by_id(c: &Connection, id: &str) -> rusqlite::Result<Option<Playlist>> {
        c.query_row(
            "SELECT id,name,created_at,updated_at FROM audio_playlists WHERE id=?",
            [id],
            playlist_row,
        )
        .optional()
    }
    pub fn insert_playlist(c: &Connection, p: &Playlist) -> rusqlite::Result<()> {
        c.execute(
            "INSERT INTO audio_playlists (id,name,created_at,updated_at) VALUES (?1,?2,?3,?4)",
            params![p.id, p.name, p.created_at, p.updated_at],
        )?;
        Ok(())
    }
    pub fn update_playlist(c: &Connection, p: &Playlist) -> rusqlite::Result<()> {
        c.execute(
            "UPDATE audio_playlists SET name=?1,updated_at=?2 WHERE id=?3",
            params![p.name, p.updated_at, p.id],
        )?;
        Ok(())
    }
    pub fn delete_playlist(c: &Connection, id: &str) -> rusqlite::Result<()> {
        c.execute("DELETE FROM audio_playlists WHERE id=?", [id])?;
        Ok(())
    }
    fn item_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<PlaylistItem> {
        Ok(PlaylistItem {
            id: r.get(0)?,
            playlist_id: r.get(1)?,
            audio_path: r.get(2)?,
            title: r.get(3)?,
            artist: r.get(4)?,
            duration_ms: r.get(5)?,
            sort_order: r.get(6)?,
            added_at: r.get(7)?,
            album_id: r.get(8)?,
        })
    }
    pub fn playlist_items(c: &Connection, id: &str) -> rusqlite::Result<Vec<PlaylistItem>> {
        c.prepare("SELECT id,playlist_id,audio_path,title,artist,duration_ms,sort_order,added_at,album_id FROM audio_playlist_items WHERE playlist_id=? ORDER BY sort_order")?.query_map([id],item_row)?.collect()
    }
    pub fn playlist_count(c: &Connection, id: &str) -> rusqlite::Result<usize> {
        c.query_row(
            "SELECT COUNT(*) FROM audio_playlist_items WHERE playlist_id=?",
            [id],
            |r| r.get(0),
        )
    }
    pub fn playlist_position(
        c: &Connection,
        id: &str,
        path: &str,
    ) -> rusqlite::Result<Option<i64>> {
        c.query_row("SELECT (SELECT COUNT(*) FROM audio_playlist_items AS earlier WHERE earlier.playlist_id=item.playlist_id AND earlier.sort_order<item.sort_order) FROM audio_playlist_items AS item WHERE item.playlist_id=?1 AND item.audio_path=?2",params![id,path],|r|r.get(0)).optional()
    }
    pub fn playlist_items_page(
        c: &Connection,
        id: &str,
        offset: i64,
        limit: i64,
    ) -> rusqlite::Result<Vec<PlaylistItem>> {
        c.prepare("SELECT id,playlist_id,audio_path,title,artist,duration_ms,sort_order,added_at,album_id FROM audio_playlist_items WHERE playlist_id=?1 ORDER BY sort_order LIMIT ?2 OFFSET ?3")?.query_map(params![id,limit,offset],item_row)?.collect()
    }
    pub fn all_playlist_items(c: &Connection) -> rusqlite::Result<Vec<PlaylistItem>> {
        c.prepare("SELECT id,playlist_id,audio_path,title,artist,duration_ms,sort_order,added_at,album_id FROM audio_playlist_items")?.query_map([],item_row)?.collect()
    }
    pub fn insert_playlist_item(c: &Connection, p: &PlaylistItem) -> rusqlite::Result<()> {
        c.execute("INSERT INTO audio_playlist_items (id,playlist_id,audio_path,title,artist,duration_ms,sort_order,added_at,album_id) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![p.id,p.playlist_id,p.audio_path,p.title,p.artist,p.duration_ms,p.sort_order,p.added_at,p.album_id])?;
        Ok(())
    }
    pub fn delete_playlist_items(c: &Connection, id: &str) -> rusqlite::Result<()> {
        c.execute("DELETE FROM audio_playlist_items WHERE playlist_id=?", [id])?;
        Ok(())
    }
    pub fn remove_playlist_item(c: &Connection, id: &str, path: &str) -> rusqlite::Result<()> {
        c.execute(
            "DELETE FROM audio_playlist_items WHERE playlist_id=?1 AND audio_path=?2",
            params![id, path],
        )?;
        Ok(())
    }
    pub fn playlist_item_counts(
        c: &Connection,
    ) -> rusqlite::Result<std::collections::HashMap<String, usize>> {
        c.prepare("SELECT playlist_id,COUNT(*) FROM audio_playlist_items GROUP BY playlist_id")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect()
    }
    fn history_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<PlayHistory> {
        Ok(PlayHistory {
            path: r.get(0)?,
            title: r.get(1)?,
            artist: r.get(2)?,
            duration_ms: r.get(3)?,
            play_count: r.get(4)?,
            played_at: r.get(5)?,
        })
    }
    pub fn all_history(c: &Connection) -> rusqlite::Result<Vec<PlayHistory>> {
        c.prepare("SELECT path,title,artist,duration_ms,play_count,played_at FROM audio_play_history ORDER BY played_at DESC,rowid DESC")?.query_map([],history_row)?.collect()
    }
    pub fn upsert_history(c: &Connection, p: &PlayHistory) -> rusqlite::Result<()> {
        c.execute("INSERT INTO audio_play_history (path,title,artist,duration_ms,play_count,played_at) VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(path) DO UPDATE SET title=?2,artist=?3,duration_ms=?4,play_count=?5,played_at=?6",params![p.path,p.title,p.artist,p.duration_ms,p.play_count,p.played_at])?;
        Ok(())
    }
    pub fn history_by_path(c: &Connection, path: &str) -> rusqlite::Result<Option<PlayHistory>> {
        c.query_row("SELECT path,title,artist,duration_ms,play_count,played_at FROM audio_play_history WHERE path=?",[path],history_row).optional()
    }
    pub fn trim_history(c: &Connection, keep: usize) -> rusqlite::Result<()> {
        c.execute("DELETE FROM audio_play_history WHERE path NOT IN (SELECT path FROM audio_play_history ORDER BY played_at DESC,rowid DESC LIMIT ?1)",[keep as i64])?;
        Ok(())
    }
    pub fn remove_playlist_items_by_paths(
        c: &Connection,
        paths: &[String],
    ) -> rusqlite::Result<()> {
        c.execute("DELETE FROM audio_playlist_items WHERE audio_path IN (SELECT value FROM json_each(?1))",[serde_json::to_string(paths).unwrap()])?;
        Ok(())
    }
    pub fn remove_history(c: &Connection, paths: &[String]) -> rusqlite::Result<()> {
        c.execute(
            "DELETE FROM audio_play_history WHERE path IN (SELECT value FROM json_each(?1))",
            [serde_json::to_string(paths).unwrap()],
        )?;
        Ok(())
    }
    pub fn remove_queue_paths(c: &Connection, paths: &[String]) -> rusqlite::Result<()> {
        c.execute(
            "DELETE FROM audio_queue_items WHERE path IN (SELECT value FROM json_each(?1))",
            [serde_json::to_string(paths).unwrap()],
        )?;
        Ok(())
    }
}

pub fn get_source(db: &Db) -> rusqlite::Result<QueueSource> {
    db.with_conn(|c| io::get_source(c))
}
pub fn save_source(db: &Db, src: &QueueSource) -> rusqlite::Result<()> {
    db.with_conn(|c| io::save_source(c, src))
}
pub fn all_queue_items(db: &Db) -> rusqlite::Result<Vec<QueueItem>> {
    db.with_conn(|c| io::all_queue_items(c))
}
pub fn queue_item_by_path(db: &Db, path: &str) -> rusqlite::Result<Option<QueueItem>> {
    db.with_conn(|c| io::queue_item_by_path(c, path))
}
pub fn replace_queue_items(db: &Db, items: &[QueueItem]) -> rusqlite::Result<()> {
    db.with_conn(|c| {
        let tx = c.unchecked_transaction()?;
        io::replace_queue_items(&tx, items)?;
        tx.commit()
    })
}
pub fn remove_queue_item(db: &Db, path: &str) -> rusqlite::Result<()> {
    db.with_conn(|c| io::remove_queue_item(c, path))
}
pub fn all_playlists(db: &Db) -> rusqlite::Result<Vec<Playlist>> {
    db.with_conn(|c| io::all_playlists(c))
}
pub fn playlist_by_id(db: &Db, id: &str) -> rusqlite::Result<Option<Playlist>> {
    db.with_conn(|c| io::playlist_by_id(c, id))
}
pub fn insert_playlist(db: &Db, p: &Playlist) -> rusqlite::Result<()> {
    db.with_conn(|c| io::insert_playlist(c, p))
}
pub fn update_playlist(db: &Db, p: &Playlist) -> rusqlite::Result<()> {
    db.with_conn(|c| io::update_playlist(c, p))
}
pub fn delete_playlist(db: &Db, id: &str) -> rusqlite::Result<()> {
    db.with_conn(|c| io::delete_playlist(c, id))
}
pub fn playlist_items(db: &Db, id: &str) -> rusqlite::Result<Vec<PlaylistItem>> {
    db.with_conn(|c| io::playlist_items(c, id))
}
pub fn all_playlist_items(db: &Db) -> rusqlite::Result<Vec<PlaylistItem>> {
    db.with_conn(|c| io::all_playlist_items(c))
}
pub fn insert_playlist_item(db: &Db, p: &PlaylistItem) -> rusqlite::Result<()> {
    db.with_conn(|c| io::insert_playlist_item(c, p))
}
pub fn delete_playlist_items(db: &Db, id: &str) -> rusqlite::Result<()> {
    db.with_conn(|c| io::delete_playlist_items(c, id))
}
pub fn remove_playlist_item(db: &Db, id: &str, path: &str) -> rusqlite::Result<()> {
    db.with_conn(|c| io::remove_playlist_item(c, id, path))
}
pub fn playlist_item_counts(db: &Db) -> rusqlite::Result<std::collections::HashMap<String, usize>> {
    db.with_conn(|c| io::playlist_item_counts(c))
}
pub fn all_history(db: &Db) -> rusqlite::Result<Vec<PlayHistory>> {
    db.with_conn(|c| io::all_history(c))
}
pub fn upsert_history(db: &Db, p: &PlayHistory) -> rusqlite::Result<()> {
    db.with_conn(|c| io::upsert_history(c, p))
}
pub fn history_by_path(db: &Db, path: &str) -> rusqlite::Result<Option<PlayHistory>> {
    db.with_conn(|c| io::history_by_path(c, path))
}
pub fn trim_history(db: &Db, keep: usize) -> rusqlite::Result<()> {
    db.with_conn(|c| io::trim_history(c, keep))
}
pub fn remove_playlist_items_by_paths(db: &Db, paths: &[String]) -> rusqlite::Result<()> {
    db.with_conn(|c| io::remove_playlist_items_by_paths(c, paths))
}
pub fn remove_history(db: &Db, paths: &[String]) -> rusqlite::Result<()> {
    db.with_conn(|c| io::remove_history(c, paths))
}
pub fn remove_queue_paths(db: &Db, paths: &[String]) -> rusqlite::Result<()> {
    db.with_conn(|c| io::remove_queue_paths(c, paths))
}
pub fn playlist_count(db: &Db, id: &str) -> rusqlite::Result<usize> {
    db.with_conn(|c| io::playlist_count(c, id))
}
pub fn playlist_position(db: &Db, id: &str, path: &str) -> rusqlite::Result<Option<i64>> {
    db.with_conn(|c| io::playlist_position(c, id, path))
}
pub fn playlist_items_page(
    db: &Db,
    id: &str,
    offset: i64,
    limit: i64,
) -> rusqlite::Result<Vec<PlaylistItem>> {
    db.with_conn(|c| io::playlist_items_page(c, id, offset, limit))
}

#[cfg(test)]
#[path = "../../../tests/unit/library/db/audio_queue.rs"]
mod tests;
