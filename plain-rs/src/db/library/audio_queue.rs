//! Row IO for the five audio tables plus `library_prefs`. One module per
//! plain-app Room table; the playback-order state machine lives in
//! [`crate::library::audio_queue`].

use rusqlite::params;

use super::LibraryDb;
/// Play history is trimmed to this many rows once it exceeds 5/4 of it.
pub const HISTORY_KEEP: usize = 200;

/// Where the queue draws its tracks from (stored as the plain-app enum
/// name: NONE / PLAYLIST / LIBRARY).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QueueSourceKind {
    #[default]
    None,
    Playlist,
    Library,
}

impl QueueSourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            QueueSourceKind::None => "NONE",
            QueueSourceKind::Playlist => "PLAYLIST",
            QueueSourceKind::Library => "LIBRARY",
        }
    }

    pub fn parse_stored(s: &str) -> Self {
        match s {
            "PLAYLIST" => QueueSourceKind::Playlist,
            "LIBRARY" => QueueSourceKind::Library,
            _ => QueueSourceKind::None,
        }
    }
}

/// The single queue-source row (plain-app `DAudioQueueSource`, id = 1).
#[derive(Clone, Debug, PartialEq)]
pub struct QueueSource {
    pub source: QueueSourceKind,
    pub playlist_id: String,
    pub current_path: String,
    /// Position of the current track inside the source, cached for fast
    /// skips. -1 = unknown (manual jump), re-located lazily on next skip.
    pub current_index: i64,
    /// `FileSortBy` name captured when a LIBRARY source was set.
    pub sort_by: String,
}

impl Default for QueueSource {
    fn default() -> Self {
        Self {
            source: QueueSourceKind::None,
            playlist_id: String::new(),
            current_path: String::new(),
            // Matches the SQL column default: unknown position.
            current_index: -1,
            sort_by: String::new(),
        }
    }
}

/// One manual-queue row ("play next" / "add to queue").
#[derive(Clone, Debug, PartialEq)]
pub struct QueueItem {
    pub path: String,
    pub sort_order: i64,
    pub title: String,
    pub artist: String,
    pub duration_secs: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlaylistItem {
    pub id: String,
    pub playlist_id: String,
    pub audio_path: String,
    // Snapshot columns let lists render without touching the media index;
    // playback still resolves the file live from audio_path.
    pub title: String,
    pub artist: String,
    pub duration_secs: i64,
    pub sort_order: i64,
    pub added_at: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayHistory {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub duration_secs: i64,
    pub play_count: i64,
    pub played_at: String,
}

// ---------------------------------------------------------------------------
// Queue source / prefs
// ---------------------------------------------------------------------------

pub fn get_source(db: &LibraryDb) -> QueueSource {
    db.with_conn(|conn| {
        let mut stmt = match conn.prepare(
            "SELECT source,playlist_id,current_path,current_index,sort_by FROM audio_queue_source WHERE id=1",
        ) {
            Ok(s) => s,
            Err(_) => return QueueSource::default(),
        };
        stmt.query_row(params![], |row| {
            Ok(QueueSource {
                source: QueueSourceKind::parse_stored(&row.get::<_, String>(0)?),
                playlist_id: row.get(1)?,
                current_path: row.get(2)?,
                current_index: row.get(3)?,
                sort_by: row.get(4)?,
            })
        })
        .unwrap_or_default()
    })
}

pub fn save_source(db: &LibraryDb, src: &QueueSource) {
    db.with_conn(|conn| {
        let _ = conn.execute(
            "INSERT INTO audio_queue_source (id,source,playlist_id,current_path,current_index,sort_by)
             VALUES (1,?1,?2,?3,?4,?5)
             ON CONFLICT(id) DO UPDATE SET source=?1,playlist_id=?2,current_path=?3,current_index=?4,sort_by=?5",
            params![
                src.source.as_str(),
                src.playlist_id,
                src.current_path,
                src.current_index,
                src.sort_by
            ],
        );
    })
}

/// Read a `library_prefs` value.
pub fn get_pref(db: &LibraryDb, key: &str) -> Option<String> {
    db.with_conn(|conn| {
        conn.query_row(
            "SELECT value FROM library_prefs WHERE key=?",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .ok()
    })
}

/// Write a `library_prefs` value.
pub fn set_pref(db: &LibraryDb, key: &str, value: &str) {
    db.with_conn(|conn| {
        let _ = conn.execute(
            "INSERT INTO library_prefs (key,value) VALUES (?1,?2)
             ON CONFLICT(key) DO UPDATE SET value=?2",
            params![key, value],
        );
    })
}

// ---------------------------------------------------------------------------
// Manual queue items
// ---------------------------------------------------------------------------

/// All manual queue items in sort_order order.
pub fn all_queue_items(db: &LibraryDb) -> Vec<QueueItem> {
    db.with_conn(|conn| {
        let mut stmt = match conn
            .prepare("SELECT path,sort_order,title,artist,duration_secs FROM audio_queue_items ORDER BY sort_order ASC")
        {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![], row_to_queue_item)
            .ok()
            .map(|iter| iter.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    })
}

fn row_to_queue_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueueItem> {
    Ok(QueueItem {
        path: row.get(0)?,
        sort_order: row.get(1)?,
        title: row.get(2)?,
        artist: row.get(3)?,
        duration_secs: row.get(4)?,
    })
}

pub fn queue_item_by_path(db: &LibraryDb, path: &str) -> Option<QueueItem> {
    db.with_conn(|conn| {
        conn.query_row(
            "SELECT path,sort_order,title,artist,duration_secs FROM audio_queue_items WHERE path=?",
            params![path],
            row_to_queue_item,
        )
        .ok()
    })
}

/// Replace the whole manual queue with `items` (dense sort orders).
pub fn replace_queue_items(db: &LibraryDb, items: &[QueueItem]) {
    db.with_conn(|conn| {
        let tx = match conn.unchecked_transaction() {
            Ok(t) => t,
            Err(_) => return,
        };
        if tx.execute("DELETE FROM audio_queue_items", params![]).is_err() {
            return;
        }
        for (i, item) in items.iter().enumerate() {
            if tx
                .execute(
                    "INSERT INTO audio_queue_items (path,sort_order,title,artist,duration_secs) VALUES (?1,?2,?3,?4,?5)",
                    params![item.path, i as i64, item.title, item.artist, item.duration_secs],
                )
                .is_err()
            {
                return;
            }
        }
        let _ = tx.commit();
    })
}

pub fn remove_queue_item(db: &LibraryDb, path: &str) {
    db.with_conn(|conn| {
        let _ = conn.execute("DELETE FROM audio_queue_items WHERE path=?", params![path]);
    })
}

// ---------------------------------------------------------------------------
// Playlists
// ---------------------------------------------------------------------------

const PLAYLIST_COLUMNS: &str = "id,name,created_at,updated_at";

/// All playlists, most recently updated first (Room DAO order). Ties
/// (same updated_at millisecond) break on id for a deterministic order.
pub fn all_playlists(db: &LibraryDb) -> Vec<Playlist> {
    db.with_conn(|conn| {
        let mut stmt = match conn.prepare(&format!(
            "SELECT {PLAYLIST_COLUMNS} FROM audio_playlists ORDER BY updated_at DESC, rowid DESC"
        )) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![], row_to_playlist)
            .ok()
            .map(|iter| iter.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    })
}

fn row_to_playlist(row: &rusqlite::Row<'_>) -> rusqlite::Result<Playlist> {
    Ok(Playlist {
        id: row.get(0)?,
        name: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
    })
}

pub fn playlist_by_id(db: &LibraryDb, id: &str) -> Option<Playlist> {
    db.with_conn(|conn| {
        conn.query_row(
            &format!("SELECT {PLAYLIST_COLUMNS} FROM audio_playlists WHERE id=?"),
            params![id],
            row_to_playlist,
        )
        .ok()
    })
}

pub fn insert_playlist(db: &LibraryDb, pl: &Playlist) {
    db.with_conn(|conn| {
        let _ = conn.execute(
            "INSERT INTO audio_playlists (id,name,created_at,updated_at) VALUES (?1,?2,?3,?4)",
            params![pl.id, pl.name, pl.created_at, pl.updated_at],
        );
    })
}

pub fn update_playlist(db: &LibraryDb, pl: &Playlist) {
    db.with_conn(|conn| {
        let _ = conn.execute(
            "UPDATE audio_playlists SET name=?1,updated_at=?2 WHERE id=?3",
            params![pl.name, pl.updated_at, pl.id],
        );
    })
}

pub fn delete_playlist(db: &LibraryDb, id: &str) {
    db.with_conn(|conn| {
        let _ = conn.execute("DELETE FROM audio_playlists WHERE id=?", params![id]);
    })
}

// ---------------------------------------------------------------------------
// Playlist items
// ---------------------------------------------------------------------------

const ITEM_COLUMNS: &str =
    "id,playlist_id,audio_path,title,artist,duration_secs,sort_order,added_at";

/// One playlist's items in sort_order order (sort orders are dense: == index).
pub fn playlist_items(db: &LibraryDb, playlist_id: &str) -> Vec<PlaylistItem> {
    db.with_conn(|conn| {
        let mut stmt = match conn.prepare(&format!(
            "SELECT {ITEM_COLUMNS} FROM audio_playlist_items WHERE playlist_id=? ORDER BY sort_order ASC"
        )) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![playlist_id], row_to_playlist_item)
            .ok()
            .map(|iter| iter.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    })
}

/// All playlist items across every playlist (cascade cleanup scans these).
pub fn all_playlist_items(db: &LibraryDb) -> Vec<PlaylistItem> {
    db.with_conn(|conn| {
        let mut stmt =
            match conn.prepare(&format!("SELECT {ITEM_COLUMNS} FROM audio_playlist_items")) {
                Ok(s) => s,
                Err(_) => return vec![],
            };
        stmt.query_map(params![], row_to_playlist_item)
            .ok()
            .map(|iter| iter.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    })
}

fn row_to_playlist_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<PlaylistItem> {
    Ok(PlaylistItem {
        id: row.get(0)?,
        playlist_id: row.get(1)?,
        audio_path: row.get(2)?,
        title: row.get(3)?,
        artist: row.get(4)?,
        duration_secs: row.get(5)?,
        sort_order: row.get(6)?,
        added_at: row.get(7)?,
    })
}

pub fn insert_playlist_item(db: &LibraryDb, item: &PlaylistItem) {
    db.with_conn(|conn| {
        let _ = conn.execute(
            "INSERT INTO audio_playlist_items (id,playlist_id,audio_path,title,artist,duration_secs,sort_order,added_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                item.id,
                item.playlist_id,
                item.audio_path,
                item.title,
                item.artist,
                item.duration_secs,
                item.sort_order,
                item.added_at
            ],
        );
    })
}

pub fn delete_playlist_items(db: &LibraryDb, playlist_id: &str) {
    db.with_conn(|conn| {
        let _ = conn.execute(
            "DELETE FROM audio_playlist_items WHERE playlist_id=?",
            params![playlist_id],
        );
    });
}

pub fn remove_playlist_item(db: &LibraryDb, playlist_id: &str, path: &str) {
    db.with_conn(|conn| {
        let _ = conn.execute(
            "DELETE FROM audio_playlist_items WHERE playlist_id=? AND audio_path=?",
            params![playlist_id, path],
        );
    })
}

/// Live item count per playlist id (playlists with none are absent).
pub fn playlist_item_counts(db: &LibraryDb) -> std::collections::HashMap<String, usize> {
    db.with_conn(|conn| {
        let mut stmt = match conn
            .prepare("SELECT playlist_id, COUNT(*) FROM audio_playlist_items GROUP BY playlist_id")
        {
            Ok(s) => s,
            Err(_) => return std::collections::HashMap::new(),
        };
        stmt.query_map(params![], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, usize>(1)?))
        })
        .ok()
        .map(|iter| iter.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    })
}

/// Remove playlist items whose `audio_path` is in `paths`, across every
/// playlist (media deleted / trashed cascade).
pub fn remove_playlist_items_by_paths(db: &LibraryDb, paths: &[String]) {
    remove_by_paths(
        db,
        "DELETE FROM audio_playlist_items WHERE audio_path",
        paths,
    );
}

// ---------------------------------------------------------------------------
// Play history
// ---------------------------------------------------------------------------

const HISTORY_COLUMNS: &str = "path,title,artist,duration_secs,play_count,played_at";

/// All history rows, newest first. `rowid` breaks same-millisecond ties
/// in insertion order, so the newest insert always ranks first.
pub fn all_history(db: &LibraryDb) -> Vec<PlayHistory> {
    db.with_conn(|conn| {
        let mut stmt = match conn.prepare(&format!(
            "SELECT {HISTORY_COLUMNS} FROM audio_play_history ORDER BY played_at DESC, rowid DESC"
        )) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![], row_to_history)
            .ok()
            .map(|iter| iter.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    })
}

fn row_to_history(row: &rusqlite::Row<'_>) -> rusqlite::Result<PlayHistory> {
    Ok(PlayHistory {
        path: row.get(0)?,
        title: row.get(1)?,
        artist: row.get(2)?,
        duration_secs: row.get(3)?,
        play_count: row.get(4)?,
        played_at: row.get(5)?,
    })
}

/// Upsert one play record (bumping play_count) and trim the table to
/// [`HISTORY_KEEP`] newest rows once it exceeds 5/4 of that.
pub fn upsert_history(db: &LibraryDb, row: &PlayHistory) {
    db.with_conn(|conn| {
        let _ = conn.execute(
            "INSERT INTO audio_play_history (path,title,artist,duration_secs,play_count,played_at) \
             VALUES (?1,?2,?3,?4,?5,?6) \
             ON CONFLICT(path) DO UPDATE SET title=?2,artist=?3,duration_secs=?4,play_count=?5,played_at=?6",
            params![
                row.path,
                row.title,
                row.artist,
                row.duration_secs,
                row.play_count,
                row.played_at
            ],
        );
    })
}

pub fn history_by_path(db: &LibraryDb, path: &str) -> Option<PlayHistory> {
    db.with_conn(|conn| {
        conn.query_row(
            &format!("SELECT {HISTORY_COLUMNS} FROM audio_play_history WHERE path=?"),
            params![path],
            row_to_history,
        )
        .ok()
    })
}

/// Delete every history row except the newest `keep` (by played_at, then
/// insertion order).
pub fn trim_history(db: &LibraryDb, keep: usize) {
    db.with_conn(|conn| {
        let _ = conn.execute(
            "DELETE FROM audio_play_history WHERE path NOT IN \
             (SELECT path FROM audio_play_history ORDER BY played_at DESC, rowid DESC LIMIT ?1)",
            params![keep as i64],
        );
    })
}

/// Remove history rows by path (media deleted / trashed cascade).
pub fn remove_history(db: &LibraryDb, paths: &[String]) {
    remove_by_paths(db, "DELETE FROM audio_play_history WHERE path", paths);
}

/// Delete rows whose column matches one of `paths`, via an IN clause.
pub fn remove_by_paths(db: &LibraryDb, delete_prefix: &str, paths: &[String]) {
    if paths.is_empty() {
        return;
    }
    db.with_conn(|conn| {
        let placeholders = (1..=paths.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("{delete_prefix} IN ({placeholders})");
        let _ = conn.execute(&sql, rusqlite::params_from_iter(paths.iter()));
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/library/db/audio_queue.rs"]
mod tests;
