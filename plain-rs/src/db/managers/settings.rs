use rusqlite::params;

use crate::db::Db;

pub fn get_setting(db: &Db, key: &str) -> Option<String> {
    db.with_conn(|conn| {
        conn.query_row(
            "SELECT value FROM settings WHERE key=?",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .ok()
    })
}

pub fn set_setting(db: &Db, key: &str, value: &str) {
    db.with_conn(|conn| {
        let _ = conn.execute(
            "INSERT INTO settings (key,value) VALUES (?1,?2)
             ON CONFLICT(key) DO UPDATE SET value=?2",
            params![key, value],
        );
    })
}
