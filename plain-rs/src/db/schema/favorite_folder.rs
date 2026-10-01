use rusqlite::Connection;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS favorite_folders (
                root_path     TEXT NOT NULL,
                relative_path TEXT NOT NULL,
                alias         TEXT,
                PRIMARY KEY (root_path, relative_path)
            );",
    )
}
