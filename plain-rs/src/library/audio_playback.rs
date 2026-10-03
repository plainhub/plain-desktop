use crate::{
    db::Db,
    library::{LibraryError, LibraryResult},
};
use rusqlite::{Connection, OptionalExtension, params};

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Playback {
    pub path: String,
    pub position_ms: i64,
    pub revision: i64,
}
fn get(c: &Connection) -> rusqlite::Result<Playback> {
    Ok(c.query_row(
        "SELECT path,position_ms,revision FROM audio_playback WHERE id=1",
        [],
        |r| {
            Ok(Playback {
                path: r.get(0)?,
                position_ms: r.get(1)?,
                revision: r.get(2)?,
            })
        },
    )
    .optional()?
    .unwrap_or_default())
}
pub(crate) fn sync_source(c: &Connection, path: &str) -> rusqlite::Result<()> {
    let old = get(c)?;
    if old.path == path {
        return Ok(());
    }
    let revision = old
        .revision
        .checked_add(1)
        .ok_or(rusqlite::Error::IntegralValueOutOfRange(2, old.revision))?;
    c.execute("INSERT INTO audio_playback (id,path,position_ms,revision) VALUES (1,?1,0,?2) ON CONFLICT(id) DO UPDATE SET path=?1,position_ms=0,revision=?2,load_revision=-1", params![path,revision])?;
    Ok(())
}
pub fn snapshot(db: &Db) -> LibraryResult<Playback> {
    Ok(db.with_conn(get)?)
}
pub fn invalidate(db: &Db) -> LibraryResult<Playback> {
    db.with_conn(|c| {
        let tx = c.unchecked_transaction()?;
        let mut row = get(&tx)?;
        row.revision = row
            .revision
            .checked_add(1)
            .ok_or_else(|| LibraryError::Other("audio revision overflow".into()))?;
        write(&tx, &row)?;
        tx.commit()?;
        Ok(row)
    })
}
pub fn seek(db: &Db, position_ms: i64) -> LibraryResult<Playback> {
    if position_ms < 0 {
        return Err(LibraryError::Other("negative audio position".into()));
    }
    db.with_conn(|c| {
        let tx = c.unchecked_transaction()?;
        let mut row = get(&tx)?;
        if row.path.is_empty() {
            return Err(LibraryError::Other("no selected audio".into()));
        }
        row.position_ms = position_ms;
        row.revision = row
            .revision
            .checked_add(1)
            .ok_or_else(|| LibraryError::Other("audio revision overflow".into()))?;
        write(&tx, &row)?;
        tx.commit()?;
        Ok(row)
    })
}
pub fn report(db: &Db, path: &str, revision: i64, position_ms: i64) -> LibraryResult<bool> {
    if revision < 0 || position_ms < 0 || path.is_empty() {
        return Err(LibraryError::Other("invalid audio progress".into()));
    }
    Ok(db.with_conn(|c| {
        c.execute(
            "UPDATE audio_playback SET position_ms=?1 WHERE id=1 AND path=?2 AND revision=?3",
            params![position_ms, path, revision],
        )
    })? > 0)
}
pub fn prepare_track(
    db: &Db,
    track: &super::audio_queue::AudioTrack,
    enqueue: bool,
) -> LibraryResult<Playback> {
    db.with_conn(|c| {
        let tx = c.unchecked_transaction()?;
        if enqueue {
            super::audio_queue::enqueue_conn(&tx, std::slice::from_ref(track), false)?;
        }
        let mut source = crate::db::audio_queue::io::get_source(&tx)?;
        source.current_path = track.path.clone();
        crate::db::audio_queue::io::save_source(&tx, &source)?;
        let mut row = get(&tx)?;
        row.position_ms = 0;
        row.revision = row
            .revision
            .checked_add(1)
            .ok_or_else(|| LibraryError::Other("audio revision overflow".into()))?;
        write(&tx, &row)?;
        tx.commit()?;
        Ok(row)
    })
}
pub fn loaded(db: &Db, path: &str, revision: i64) -> LibraryResult<()> {
    let changed=db.with_conn(|c|c.execute("UPDATE audio_playback SET load_revision=?1,started_revision=-1 WHERE id=1 AND revision=?1 AND path=?2",params![revision,path]))?;
    if changed == 0 {
        return Err(LibraryError::Other(
            "stale engine load acknowledgement".into(),
        ));
    }
    Ok(())
}

pub fn started(
    db: &Db,
    track: &super::audio_queue::AudioTrack,
    revision: i64,
) -> LibraryResult<bool> {
    if track.path.is_empty() || track.duration_ms < 0 || revision < 0 {
        return Err(LibraryError::Other("invalid playback start".into()));
    }
    db.with_conn(|c| {
        let tx=c.unchecked_transaction()?;
        let changed=tx.execute("UPDATE audio_playback SET started_revision=?1 WHERE id=1 AND load_revision=?1 AND path=?2 AND started_revision<>?1",params![revision,track.path])?;
        if changed>0 { super::audio_queue::record_history_conn(&tx,&track.path,&track.title,&track.artist,track.duration_ms)?; }
        tx.commit()?;
        Ok(changed>0)
    })
}

fn write(c: &Connection, row: &Playback) -> rusqlite::Result<()> {
    c.execute("INSERT INTO audio_playback (id,path,position_ms,revision) VALUES (1,?1,?2,?3) ON CONFLICT(id) DO UPDATE SET path=?1,position_ms=?2,revision=?3",params![row.path,row.position_ms,row.revision])?;
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/library/audio_playback.rs"]
mod tests;
