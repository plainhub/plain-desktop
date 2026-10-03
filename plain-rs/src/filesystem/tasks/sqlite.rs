use super::{FileTask, Store};
use crate::db::Db;
use anyhow::{Result, bail};
use rusqlite::params;
use std::sync::Arc;
pub struct SqliteStore(pub Arc<Db>);
impl Store for SqliteStore {
    fn remove(&self, client_id: &str, id: &str) -> Result<bool> {
        Ok(self.0.with_conn(|c| c.execute("DELETE FROM file_tasks WHERE id=?1 AND client_id=?2 AND status IN ('DONE','ERROR')",params![id,client_id]))? == 1)
    }
    fn put(&self, task: &FileTask) -> Result<()> {
        let kind = serde_json::to_value(task.kind)?
            .as_str()
            .unwrap()
            .to_owned();
        let status = serde_json::to_value(task.status)?
            .as_str()
            .unwrap()
            .to_owned();
        let completed = serde_json::to_string(&task.completed_ops)?;
        let changed = self.0.with_conn(|c| c.execute("INSERT INTO file_tasks(id,client_id,type,title,status,error,total_bytes,done_bytes,total_items,done_items,created_at,updated_at,completed_ops)
            VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
            ON CONFLICT(id) DO UPDATE SET type=excluded.type,title=excluded.title,status=excluded.status,error=excluded.error,total_bytes=excluded.total_bytes,done_bytes=excluded.done_bytes,total_items=excluded.total_items,done_items=excluded.done_items,updated_at=excluded.updated_at,completed_ops=excluded.completed_ops
            WHERE file_tasks.client_id=excluded.client_id", params![task.id,task.client_id,kind,task.title,status,task.error,task.total_bytes,task.done_bytes,task.total_items,task.done_items,task.created_at.to_rfc3339(),task.updated_at.to_rfc3339(),completed]))?;
        if changed != 1 {
            bail!("file task owner mismatch");
        }
        Ok(())
    }
    fn list(&self, client_id: &str) -> Result<Vec<FileTask>> {
        self.0.with_conn(|c| -> Result<Vec<FileTask>> {
            let mut stmt = c.prepare("SELECT id,client_id,type,title,status,error,total_bytes,done_bytes,total_items,done_items,created_at,updated_at,completed_ops FROM file_tasks WHERE client_id=?1 ORDER BY updated_at DESC,id")?;
            let mut rows = stmt.query([client_id])?;
            let mut tasks = Vec::new();
            while let Some(row) = rows.next()? {
                let kind: String = row.get(2)?;
                let status: String = row.get(4)?;
                tasks.push(FileTask {
                    id:row.get(0)?,client_id:row.get(1)?,kind:serde_json::from_value(serde_json::Value::String(kind))?,title:row.get(3)?,status:serde_json::from_value(serde_json::Value::String(status))?,error:row.get(5)?,total_bytes:row.get(6)?,done_bytes:row.get(7)?,total_items:row.get(8)?,done_items:row.get(9)?,created_at:row.get::<_,String>(10)?.parse()?,updated_at:row.get::<_,String>(11)?.parse()?,completed_ops:serde_json::from_str(&row.get::<_,String>(12)?)?,last_persist:None,
                });
            }
            Ok(tasks)
        })
    }
}
#[cfg(test)]
#[path = "../../../tests/unit/filesystem/tasks/sqlite.rs"]
mod tests;
