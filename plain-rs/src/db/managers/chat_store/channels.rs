use super::super::channel::{CHANNEL_COLS, row_to_channel};
use super::{SaveMode, validate};
use crate::db::{DChannel, Db};
use anyhow::{Result, bail};
use rusqlite::{OptionalExtension, params};
pub fn get(db: &Db, id: &str) -> Result<Option<DChannel>> {
    Ok(db.with_conn(|c| {
        c.query_row(
            &format!("SELECT {CHANNEL_COLS} FROM chat_channels WHERE id=?1"),
            [id],
            row_to_channel,
        )
        .optional()
    })?)
}
pub fn all(db: &Db) -> Result<Vec<DChannel>> {
    Ok(db.with_conn(|c| {
        let mut s = c.prepare(&format!(
            "SELECT {CHANNEL_COLS} FROM chat_channels ORDER BY created_at,id"
        ))?;
        s.query_map([], row_to_channel)?
            .collect::<rusqlite::Result<Vec<_>>>()
    })?)
}
pub fn save(db: &Db, rows: &[DChannel], mode: SaveMode) -> Result<()> {
    for row in rows {
        validate(&row.id, &row.created_at, &row.updated_at)?;
        serde_json::from_str::<Vec<crate::chat::channel::messages::ChannelMember>>(&row.members)?;
        if row.version < 0 {
            bail!("negative channel version");
        }
    }
    db.with_conn(|c|->Result<()>{ let tx=c.unchecked_transaction()?;
 for row in rows {
  let sql=match mode { SaveMode::Insert=>"INSERT INTO chat_channels(id,name,owner_id,members,key,version,status,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)", SaveMode::Update=>"UPDATE chat_channels SET name=?2,owner_id=?3,members=?4,key=?5,version=?6,status=?7,created_at=?8,updated_at=?9 WHERE id=?1",SaveMode::Upsert=>"INSERT INTO chat_channels(id,name,owner_id,members,key,version,status,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(id) DO UPDATE SET name=excluded.name,owner_id=excluded.owner_id,members=excluded.members,key=excluded.key,version=excluded.version,status=excluded.status,updated_at=excluded.updated_at" };
  let changed=tx.execute(sql,params![row.id,row.name,row.owner_id,row.members,row.key,row.version,row.status,row.created_at,row.updated_at])?;
  if changed!=1 { bail!("channels record missing"); }
 } tx.commit()?; Ok(()) })
}
pub fn delete(db: &Db, ids: &[String]) -> Result<usize> {
    db.with_conn(|c| -> Result<usize> {
        let tx = c.unchecked_transaction()?;
        let mut n = 0;
        for id in ids {
            n += tx.execute("DELETE FROM chat_channels WHERE id=?1", [id])?;
        }
        tx.commit()?;
        Ok(n)
    })
}
pub fn patch(db: &Db, before: &DChannel, after: &DChannel) -> Result<DChannel> {
    if before.id != after.id {
        bail!("channel patch identity mismatch");
    }
    validate(&after.id, &after.created_at, &after.updated_at)?;
    serde_json::from_str::<Vec<crate::chat::channel::messages::ChannelMember>>(&after.members)?;
    if after.version < before.version {
        bail!("channel version moved backwards");
    }
    db.with_conn(|c| -> Result<DChannel> {
        let tx=c.unchecked_transaction()?;
        if tx.execute("UPDATE chat_channels SET name=?2,owner_id=?3,members=?4,key=?5,version=?6,status=?7,updated_at=?8 WHERE id=?1 AND version=?9 AND updated_at=?10",params![after.id,after.name,after.owner_id,after.members,after.key,after.version,after.status,after.updated_at,before.version,before.updated_at])?!=1 { bail!("channel changed during mutation"); }
        let row=tx.query_row(&format!("SELECT {CHANNEL_COLS} FROM chat_channels WHERE id=?1"),[&after.id],row_to_channel)?;
        tx.commit()?;
        Ok(row)
    })
}
