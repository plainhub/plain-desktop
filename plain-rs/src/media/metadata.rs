use crate::db::Db;
use crate::db::MediaItemRow;
use crate::error::DomainError;
use crate::error::DomainResult;
use rusqlite::params;
pub fn all(db: &Db) -> DomainResult<Vec<MediaItemRow>> {
    Ok(db.media_item_list()?)
}
pub fn save(db: &Db, kind: &str, id: &str, duration_ms: i64) -> DomainResult<()> {
    if !matches!(kind, "audio" | "video") || id.is_empty() || duration_ms < 0 {
        return Err(DomainError::Other("invalid media duration".into()));
    }
    db.media_item_upsert(&MediaItemRow {
        media_type: kind.into(),
        media_id: id.into(),
        duration_ms,
        updated_at: crate::utils::dbtime::now_iso_millis(),
    })?;
    Ok(())
}
pub fn delete(db: &Db, kind: &str, ids: &[String]) -> DomainResult<usize> {
    let json = serde_json::to_string(ids).map_err(|e| DomainError::Other(e.to_string()))?;
    Ok(db.with_conn(|c|c.execute("DELETE FROM media_item WHERE media_type=?1 AND media_id IN (SELECT value FROM json_each(?2))",params![kind,json]))?)
}
#[cfg(test)]
#[path = "../../tests/unit/media/metadata.rs"]
mod tests;
