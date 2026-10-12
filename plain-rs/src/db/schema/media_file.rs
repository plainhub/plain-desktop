use rusqlite::Connection;

pub(crate) fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS media_files (
            id TEXT PRIMARY KEY NOT NULL,
            data_type INTEGER NOT NULL CHECK(data_type IN (1,2,3,24)),
            path TEXT NOT NULL CHECK(length(path)>0),
            mime_type TEXT NOT NULL,
            file_name TEXT NOT NULL,
            title TEXT NOT NULL,
            artist TEXT NOT NULL,
            bucket_id TEXT NOT NULL,
            size_bytes INTEGER NOT NULL CHECK(size_bytes>=0),
            duration_ms INTEGER NOT NULL CHECK(duration_ms>=0),
            album_file_id TEXT NOT NULL,
            width INTEGER NOT NULL CHECK(width>=0),
            height INTEGER NOT NULL CHECK(height>=0),
            rotation INTEGER NOT NULL,
            taken_at TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            is_favorite INTEGER NOT NULL DEFAULT 0 CHECK(is_favorite IN (0,1)),
            is_trashed INTEGER NOT NULL DEFAULT 0 CHECK(is_trashed IN (0,1)),
            original_path TEXT
        );
        CREATE UNIQUE INDEX IF NOT EXISTS uidx_media_files_data_type_path ON media_files(data_type,path);
        CREATE INDEX IF NOT EXISTS idx_media_files_data_type_is_trashed_updated_at ON media_files(data_type,is_trashed,updated_at);",
    )
}
