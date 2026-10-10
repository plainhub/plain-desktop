use crate::db::Db;
use crate::db::VideoPlayProgressRow;

pub fn save(
    db: &Db,
    media_id: &str,
    position_ms: i64,
) -> crate::error::DomainResult<VideoPlayProgressRow> {
    if media_id.trim().is_empty() || position_ms < 0 {
        return Err(crate::error::DomainError::Other(
            "invalid video progress".into(),
        ));
    }
    let row = VideoPlayProgressRow {
        media_id: media_id.into(),
        position_ms,
        updated_at: crate::utils::dbtime::now_iso_millis(),
    };
    db.video_progress_upsert(&row)?;
    Ok(row)
}

pub fn recent(
    db: &Db,
    since: chrono::DateTime<chrono::Utc>,
) -> crate::error::DomainResult<Vec<VideoPlayProgressRow>> {
    Ok(db.video_progress_recent(&since.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))?)
}

pub fn delete(db: &Db, media_id: &str) -> crate::error::DomainResult<()> {
    if media_id.trim().is_empty() {
        return Err(crate::error::DomainError::Other("invalid media ID".into()));
    }
    db.video_progress_delete(media_id)?;
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/unit/media/videos/progress.rs"]
mod tests;
