use super::*;
use crate::filesystem::deletion::Outcome;
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize)]
pub(super) struct Intent {
    pub id: String,
    pub path: String,
    pub root: String,
    pub planned: Vec<String>,
    pub snapshot: serde_json::Value,
    pub outcome: Option<Outcome>,
}
impl Intent {
    pub fn save(&self, db: &Db) -> Result<()> {
        let payload = serde_json::to_string(self)?;
        db.with_conn(|c| c.execute("INSERT INTO file_deletions(id,payload) VALUES (?1,?2) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",rusqlite::params![self.id,payload]))?;
        Ok(())
    }
    pub fn remove(&self, db: &Db) -> Result<()> {
        db.with_conn(|c| c.execute("DELETE FROM file_deletions WHERE id=?1", [&self.id]))?;
        Ok(())
    }
}
pub(super) fn require_absent(path: &str) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
        Ok(_) => bail!("deleted path reappeared: {path}"),
    }
}
impl FileTasks {
    pub async fn recover_deletions(&self) -> Result<usize> {
        let _guard = self.deletion_lock.lock().await;
        let payloads = self.db.with_conn(|c| -> rusqlite::Result<Vec<String>> {
            c.prepare("SELECT payload FROM file_deletions ORDER BY id")?
                .query_map([], |r| r.get(0))?
                .collect()
        })?;
        let mut count = 0;
        for payload in payloads {
            let intent: Intent = serde_json::from_str(&payload)?;
            self.hooks
                .call("fileTaskAuthorize", json!({"paths":[intent.root]}))
                .await?;
            let outcome = match &intent.outcome {
                Some(outcome) => outcome.clone(),
                None => {
                    let mut paths = Vec::new();
                    for path in &intent.planned {
                        match std::fs::symlink_metadata(path) {
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                                paths.push(path.clone())
                            }
                            Err(e) => return Err(e.into()),
                            Ok(_) => {}
                        }
                    }
                    Outcome {
                        removed: paths.contains(&intent.root),
                        paths,
                        failures: Vec::new(),
                    }
                }
            };
            self.finish_deletion(&intent, &outcome).await?;
            count += 1;
        }
        Ok(count)
    }
}
#[cfg(test)]
#[path = "../../../tests/unit/content_api/file_tasks/deletion_recovery.rs"]
mod tests;
