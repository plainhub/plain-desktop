use super::{
    LibraryResult,
    audio_commands::{self, Engine},
    audio_playback,
};
use crate::{db::Db, prefs::Prefs};
use rusqlite::{OptionalExtension, params};
pub fn clear_after_move(
    db: &Db,
    prefs: &Prefs,
    engine: &mut impl Engine,
    receipt: Option<&str>,
    moved_path: Option<&str>,
) -> LibraryResult<()> {
    let current = audio_playback::snapshot(db)?;
    let pending = if let Some(id) = receipt {
        db.with_conn(|c| -> rusqlite::Result<Option<(String, i64)>> {
            if let Some(path) = moved_path {
                c.execute("INSERT INTO file_task_audio_effects(id,path,revision) VALUES(?1,?2,?3) ON CONFLICT(id) DO NOTHING",params![id,path,current.revision])?;
            }
            c.query_row("SELECT path,revision FROM file_task_audio_effects WHERE id=?1",[id],|row|Ok((row.get(0)?,row.get(1)?))).optional()
        })?
    } else {
        moved_path.map(|path| (path.to_owned(), current.revision))
    };
    let Some((path, revision)) = pending else {
        return Ok(());
    };
    if current.path.is_empty() || (current.path == path && current.revision == revision) {
        audio_commands::command(db, prefs, engine, audio_commands::Action::Clear, 0, 1.0)?;
    }
    if let Some(id) = receipt {
        db.with_conn(|c| c.execute("DELETE FROM file_task_audio_effects WHERE id=?1", [id]))?;
    }
    Ok(())
}
#[cfg(test)]
#[path = "../../tests/unit/library/file_task_audio.rs"]
mod tests;
