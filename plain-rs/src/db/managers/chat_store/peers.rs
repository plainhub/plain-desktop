use super::super::peer::{PEER_COLS, row_to_peer};
use super::{SaveMode, validate};
use crate::db::{DPeer, Db};
use anyhow::{Result, bail};
use rusqlite::{OptionalExtension, params};
pub fn get(db: &Db, id: &str) -> Result<Option<DPeer>> {
    Ok(db.with_conn(|c| {
        c.query_row(
            &format!("SELECT {PEER_COLS} FROM peers WHERE id=?1"),
            [id],
            row_to_peer,
        )
        .optional()
    })?)
}
pub fn all(db: &Db) -> Result<Vec<DPeer>> {
    Ok(db.with_conn(|c| {
        let mut s = c.prepare(&format!(
            "SELECT {PEER_COLS} FROM peers ORDER BY created_at,id"
        ))?;
        s.query_map([], row_to_peer)?
            .collect::<rusqlite::Result<Vec<_>>>()
    })?)
}
pub fn save(db: &Db, rows: &[DPeer], mode: SaveMode) -> Result<()> {
    for row in rows {
        validate(&row.id, &row.created_at, &row.updated_at)?;
    }
    db.with_conn(|c|->Result<()>{ let tx=c.unchecked_transaction()?;
 for row in rows {
  let sql=match mode { SaveMode::Insert=>"INSERT INTO peers(id,name,ip,key,public_key,status,port,device_type,token,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)", SaveMode::Update=>"UPDATE peers SET name=?2,ip=?3,key=?4,public_key=?5,status=?6,port=?7,device_type=?8,created_at=?10,updated_at=?11 WHERE id=?1",SaveMode::Upsert=>"INSERT INTO peers(id,name,ip,key,public_key,status,port,device_type,token,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) ON CONFLICT(id) DO UPDATE SET name=excluded.name,ip=excluded.ip,key=excluded.key,public_key=excluded.public_key,status=excluded.status,port=excluded.port,device_type=excluded.device_type,updated_at=excluded.updated_at" };
  let changed=tx.execute(sql,params![row.id,row.name,row.ip,row.key,row.public_key,row.status,row.port,row.device_type,row.token,row.created_at,row.updated_at])?;
  if changed!=1 { bail!("peers record missing"); }
 } tx.commit()?; Ok(()) })
}
pub fn delete(db: &Db, ids: &[String]) -> Result<usize> {
    db.with_conn(|c| -> Result<usize> {
        let tx = c.unchecked_transaction()?;
        let mut n = 0;
        for id in ids {
            n += tx.execute("DELETE FROM peers WHERE id=?1", [id])?;
        }
        tx.commit()?;
        Ok(n)
    })
}
pub fn patch(db: &Db, before: &DPeer, after: &DPeer) -> Result<Option<DPeer>> {
    if before.id != after.id {
        bail!("peer patch identity mismatch");
    }
    validate(&after.id, &after.created_at, &after.updated_at)?;
    db.with_conn(|c|->Result<Option<DPeer>>{
 let tx=c.unchecked_transaction()?;
 tx.execute("UPDATE peers SET name=CASE WHEN ?2 THEN ?3 ELSE name END,ip=CASE WHEN ?4 THEN ?5 ELSE ip END,key=CASE WHEN ?6 THEN ?7 ELSE key END,public_key=CASE WHEN ?8 THEN ?9 ELSE public_key END,status=CASE WHEN ?10 THEN ?11 ELSE status END,port=CASE WHEN ?12 THEN ?13 ELSE port END,device_type=CASE WHEN ?14 THEN ?15 ELSE device_type END,updated_at=CASE WHEN ?16 THEN ?17 ELSE updated_at END WHERE id=?1",params![before.id,before.name!=after.name,after.name,before.ip!=after.ip,after.ip,before.key!=after.key,after.key,before.public_key!=after.public_key,after.public_key,before.status!=after.status,after.status,before.port!=after.port,after.port,before.device_type!=after.device_type,after.device_type,before.updated_at!=after.updated_at,after.updated_at])?;
 let row=tx.query_row(&format!("SELECT {PEER_COLS} FROM peers WHERE id=?1"),[&before.id],row_to_peer).optional()?;
 tx.commit()?;Ok(row)
 })
}
