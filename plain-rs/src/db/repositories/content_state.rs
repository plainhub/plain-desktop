use crate::db::Db;
use rusqlite::{params, params_from_iter, types::Value};
impl Db {
    pub fn feed_entry_set_image(&self, id: &str, image: &str) -> rusqlite::Result<()> {
        self.with_conn(|c| {
            c.execute(
                "UPDATE feed_entries SET image=?2 WHERE id=?1",
                params![id, image],
            )
            .map(|_| ())
        })
    }
    pub fn feed_set_sync_status(&self, id: &str, at: &str, error: &str) -> rusqlite::Result<()> {
        self.with_conn(|c| {
            c.execute(
                "UPDATE feeds SET last_sync_at=?2,last_error=?3 WHERE id=?1",
                params![id, at, error],
            )
            .map(|_| ())
        })
    }
    pub fn feed_set_logo(&self, id: &str, logo: &str) -> rusqlite::Result<()> {
        self.with_conn(|c| {
            c.execute("UPDATE feeds SET logo=?2 WHERE id=?1", params![id, logo])
                .map(|_| ())
        })
    }
    pub fn feed_entries_mark_read(&self, ids: &[String], read: bool) -> rusqlite::Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }
        self.with_conn(|c| {
            let tx = c.unchecked_transaction()?;
            let mut count = 0;
            for chunk in ids.chunks(100) {
                let mut values = vec![Value::Integer(i64::from(read))];
                values.extend(chunk.iter().cloned().map(Value::Text));
                count += tx.execute(
                    &format!(
                        "UPDATE feed_entries SET read=? WHERE id IN ({})",
                        vec!["?"; chunk.len()].join(",")
                    ),
                    params_from_iter(values),
                )?;
            }
            tx.commit()?;
            Ok(count)
        })
    }
}
