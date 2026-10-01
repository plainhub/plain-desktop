use rusqlite::Connection;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS image_editor_projects (
                id TEXT PRIMARY KEY,
                state_b64 TEXT NOT NULL DEFAULT '',
                thumbnail TEXT,
                canvas_width INTEGER NOT NULL DEFAULT 0,
                canvas_height INTEGER NOT NULL DEFAULT 0,
                layer_count INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_image_editor_projects_updated
                ON image_editor_projects(updated_at DESC);",
    )
}
