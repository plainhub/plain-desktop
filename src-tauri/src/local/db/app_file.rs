use rusqlite::params;

use super::ChatDb;

/// Content-addressable file store record. Matches plain-app `DAppFile`.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct DAppFile {
    /// Full SHA-256 hex digest (64 chars) — primary key.
    pub id: String,
    pub size: i64,
    pub mime_type: String,
    pub real_path: String,
    pub ref_count: i32,
    /// SHA-256 hex digest of first 4 KB + last 4 KB (fast dedup probe).
    pub weak_hash: String,
    pub created_at: String,
    pub updated_at: String,
}

impl ChatDb {
    #[allow(dead_code)]
    pub fn get_app_file(&self, id: &str) -> Option<DAppFile> {
        let conn = self.0.lock().unwrap();
        conn.query_row(
            "SELECT id,size,mime_type,real_path,ref_count,weak_hash,created_at,updated_at \
             FROM app_files WHERE id=?",
            params![id],
            |row| {
                Ok(DAppFile {
                    id: row.get(0)?,
                    size: row.get(1)?,
                    mime_type: row.get(2)?,
                    real_path: row.get(3)?,
                    ref_count: row.get(4)?,
                    weak_hash: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            },
        )
        .ok()
    }

    #[allow(dead_code)]
    pub fn find_app_files_by_weak(&self, size: i64, weak_hash: &str) -> Vec<DAppFile> {
        let conn = self.0.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id,size,mime_type,real_path,ref_count,weak_hash,created_at,updated_at \
             FROM app_files WHERE size=? AND weak_hash=?",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![size, weak_hash], |row| {
            Ok(DAppFile {
                id: row.get(0)?,
                size: row.get(1)?,
                mime_type: row.get(2)?,
                real_path: row.get(3)?,
                ref_count: row.get(4)?,
                weak_hash: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .ok()
        .map(|iter| iter.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    #[allow(dead_code)]
    pub fn insert_app_file(&self, f: &DAppFile) {
        let conn = self.0.lock().unwrap();
        let _ = conn.execute(
            "INSERT OR IGNORE INTO app_files \
             (id,size,mime_type,real_path,ref_count,weak_hash,created_at,updated_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                f.id,
                f.size,
                f.mime_type,
                f.real_path,
                f.ref_count,
                f.weak_hash,
                f.created_at,
                f.updated_at
            ],
        );
    }

    #[allow(dead_code)]
    pub fn increment_app_file_ref(&self, id: &str) {
        let conn = self.0.lock().unwrap();
        let _ = conn.execute(
            "UPDATE app_files SET ref_count=ref_count+1 WHERE id=?",
            params![id],
        );
    }

    #[allow(dead_code)]
    pub fn decrement_app_file_ref(&self, id: &str) -> i32 {
        {
            let conn = self.0.lock().unwrap();
            let _ = conn.execute(
                "UPDATE app_files SET ref_count=ref_count-1 WHERE id=?",
                params![id],
            );
        }
        let conn = self.0.lock().unwrap();
        conn.query_row(
            "SELECT ref_count FROM app_files WHERE id=?",
            params![id],
            |r| r.get(0),
        )
        .unwrap_or(0)
    }

    #[allow(dead_code)]
    pub fn delete_app_file(&self, id: &str) {
        let conn = self.0.lock().unwrap();
        let _ = conn.execute("DELETE FROM app_files WHERE id=?", params![id]);
    }

    pub fn get_app_file_page(&self, limit: i32, offset: i32) -> Vec<DAppFile> {
        let conn = self.0.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id,size,mime_type,real_path,ref_count,weak_hash,created_at,updated_at \
             FROM app_files ORDER BY created_at DESC LIMIT ?1 OFFSET ?2",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![limit, offset], |row| {
            Ok(DAppFile {
                id: row.get(0)?,
                size: row.get(1)?,
                mime_type: row.get(2)?,
                real_path: row.get(3)?,
                ref_count: row.get(4)?,
                weak_hash: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .ok()
        .map(|iter| iter.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    pub fn get_all_app_files(&self) -> Vec<DAppFile> {
        let conn = self.0.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id,size,mime_type,real_path,ref_count,weak_hash,created_at,updated_at \
             FROM app_files ORDER BY created_at DESC",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![], |row| {
            Ok(DAppFile {
                id: row.get(0)?,
                size: row.get(1)?,
                mime_type: row.get(2)?,
                real_path: row.get(3)?,
                ref_count: row.get(4)?,
                weak_hash: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .ok()
        .map(|iter| iter.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    pub fn count_app_files(&self) -> i32 {
        let conn = self.0.lock().unwrap();
        conn.query_row("SELECT COUNT(*) FROM app_files", params![], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap_or(0) as i32
    }
}
