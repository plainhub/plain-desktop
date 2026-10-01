use rusqlite::Connection;

pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS image_editor_projects (
                id TEXT NOT NULL PRIMARY KEY,
                state_b64 TEXT NOT NULL,
                thumbnail TEXT,
                canvas_width INTEGER NOT NULL,
                canvas_height INTEGER NOT NULL,
                layer_count INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_image_editor_projects_updated
                ON image_editor_projects(updated_at DESC);",
    )
}
