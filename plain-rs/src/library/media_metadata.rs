use crate::{
    db::{Db, MediaItemRow},
    library::{LibraryError, LibraryResult},
};
use rusqlite::params;
pub fn all(db: &Db) -> LibraryResult<Vec<MediaItemRow>> {
    Ok(db.media_item_list()?)
}
pub fn save(db: &Db, kind: &str, id: &str, duration_ms: i64) -> LibraryResult<()> {
    if !matches!(kind, "audio" | "video") || id.is_empty() || duration_ms < 0 {
        return Err(LibraryError::Other("invalid media duration".into()));
    }
    db.media_item_upsert(&MediaItemRow {
        media_type: kind.into(),
        media_id: id.into(),
        duration_ms,
        updated_at: crate::utils::dbtime::now_iso_millis(),
    })?;
    Ok(())
}
pub fn delete(db: &Db, kind: &str, ids: &[String]) -> LibraryResult<usize> {
    let json = serde_json::to_string(ids).map_err(|e| LibraryError::Other(e.to_string()))?;
    Ok(db.with_conn(|c|c.execute("DELETE FROM media_item WHERE media_type=?1 AND media_id IN (SELECT value FROM json_each(?2))",params![kind,json]))?)
}
#[cfg(test)]
#[path = "../../tests/unit/library/media_metadata.rs"]
mod tests;
