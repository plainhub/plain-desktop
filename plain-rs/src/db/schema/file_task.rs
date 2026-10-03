use rusqlite::Connection;
pub(super) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS file_tasks (
        id TEXT NOT NULL PRIMARY KEY,
        client_id TEXT NOT NULL CHECK(length(client_id)>0),
        type TEXT NOT NULL CHECK(type IN ('COPY','MOVE')),
        title TEXT NOT NULL,
        status TEXT NOT NULL CHECK(status IN ('QUEUED','RUNNING','DONE','ERROR')),
        error TEXT NOT NULL,
        total_bytes INTEGER NOT NULL CHECK(total_bytes>=0),
        done_bytes INTEGER NOT NULL CHECK(done_bytes>=0),
        total_items INTEGER NOT NULL CHECK(total_items>=0),
        done_items INTEGER NOT NULL CHECK(done_items>=0),
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        completed_ops TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS file_task_audio_effects (id TEXT NOT NULL PRIMARY KEY, path TEXT NOT NULL, revision INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS file_task_effects (id TEXT NOT NULL PRIMARY KEY);
    CREATE INDEX IF NOT EXISTS idx_file_tasks_client_updated ON file_tasks(client_id, updated_at);",
    )
}
