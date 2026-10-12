use crate::db::{
    Db, MediaFileMetadata, MediaFileMetadataAvailability, MediaFileMetadataRead, MediaFileRow,
};
use crate::enums::DataType;
use rusqlite::{OptionalExtension, params};

pub const COLUMNS: &str = "id,data_type,path,title,artist,bucket_id,size_bytes,duration_ms,album_file_id,width,height,rotation,taken_at,created_at,updated_at,is_favorite,is_trashed,original_path";

pub fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MediaFileRow> {
    Ok(MediaFileRow {
        id: row.get(0)?,
        data_type: row.get(1)?,
        path: row.get(2)?,
        title: row.get(3)?,
        artist: row.get(4)?,
        bucket_id: row.get(5)?,
        size_bytes: row.get(6)?,
        duration_ms: row.get(7)?,
        album_file_id: row.get(8)?,
        width: row.get(9)?,
        height: row.get(10)?,
        rotation: row.get(11)?,
        taken_at: row.get(12)?,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
        is_favorite: row.get(15)?,
        is_trashed: row.get(16)?,
        original_path: row.get(17)?,
    })
}

fn kind(data_type: DataType) -> rusqlite::Result<i32> {
    data_type
        .media_type_str()
        .ok_or(rusqlite::Error::InvalidQuery)?;
    Ok(data_type.kind())
}

impl Db {
    pub fn record_media_files(
        &self,
        files: &[MediaFileMetadata],
    ) -> rusqlite::Result<Vec<MediaFileRow>> {
        self.record_media_file_inputs(
            files
                .iter()
                .map(|file| (file, MediaFileMetadataAvailability::ALL)),
        )
    }

    pub fn record_media_file_reads(
        &self,
        files: &[MediaFileMetadataRead],
    ) -> rusqlite::Result<Vec<MediaFileRow>> {
        self.record_media_file_inputs(files.iter().map(|file| (&file.metadata, file.available)))
    }

    fn record_media_file_inputs<'a>(
        &self,
        files: impl IntoIterator<Item = (&'a MediaFileMetadata, MediaFileMetadataAvailability)>,
    ) -> rusqlite::Result<Vec<MediaFileRow>> {
        self.with_conn(|conn| {
            let tx = conn.unchecked_transaction()?;
            let mut rows = Vec::new();
            for (file, available) in files {
                let data_type = kind(file.data_type)?;
                tx.execute(
                    "INSERT INTO media_files(id,data_type,path,title,artist,bucket_id,size_bytes,duration_ms,album_file_id,width,height,rotation,taken_at,created_at,updated_at,mime_type,file_name)
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)
                     ON CONFLICT(data_type,path) DO UPDATE SET title=CASE WHEN ?18 THEN excluded.title ELSE media_files.title END,artist=CASE WHEN ?18 THEN excluded.artist ELSE media_files.artist END,bucket_id=excluded.bucket_id,size_bytes=excluded.size_bytes,duration_ms=CASE WHEN ?19 THEN excluded.duration_ms ELSE media_files.duration_ms END,album_file_id=CASE WHEN ?20 THEN excluded.album_file_id ELSE media_files.album_file_id END,width=CASE WHEN ?21 THEN excluded.width ELSE media_files.width END,height=CASE WHEN ?21 THEN excluded.height ELSE media_files.height END,rotation=CASE WHEN ?22 THEN excluded.rotation ELSE media_files.rotation END,taken_at=CASE WHEN ?23 THEN excluded.taken_at ELSE media_files.taken_at END,updated_at=excluded.updated_at,mime_type=excluded.mime_type,file_name=excluded.file_name",
                    params![uuid::Uuid::new_v4().to_string(),data_type,file.path,file.title,file.artist,file.bucket_id,file.size_bytes,file.duration_ms,file.album_file_id,file.width,file.height,file.rotation,file.taken_at,file.created_at,file.updated_at,crate::utils::mime::mime_from_ext(&file.path),std::path::Path::new(&file.path).file_name().and_then(|name|name.to_str()).unwrap_or(&file.path),available.tags,available.duration,available.album_file_id,available.dimensions,available.rotation,available.taken_at],
                )?;
                rows.push(tx.query_row(&format!("SELECT {COLUMNS} FROM media_files WHERE data_type=?1 AND path=?2"),params![data_type,file.path],from_row)?);
            }
            tx.commit()?;
            Ok(rows)
        })
    }

    pub fn media_file_by_path(
        &self,
        data_type: DataType,
        path: &str,
    ) -> rusqlite::Result<Option<MediaFileRow>> {
        let data_type = kind(data_type)?;
        self.with_conn(|conn| {
            conn.query_row(
                &format!("SELECT {COLUMNS} FROM media_files WHERE data_type=?1 AND path=?2"),
                params![data_type, path],
                from_row,
            )
            .optional()
        })
    }

    /// Forget one catalogued path and everything keyed to it. A filesystem
    /// watcher reports the file is gone; without this the row and its
    /// tag, duration and embedding rows would outlive it.
    pub fn forget_media_file(&self, path: &str) -> rusqlite::Result<usize> {
        self.with_conn(|conn| {
            let tx = conn.unchecked_transaction()?;
            let mut statement = tx.prepare("SELECT data_type,id FROM media_files WHERE path=?1")?;
            let rows = statement
                .query_map([path], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            drop(statement);
            for (data_type, id) in &rows {
                tx.execute(
                    "DELETE FROM tag_relations WHERE type=?1 AND key=?2",
                    params![data_type, id],
                )?;
                tx.execute("DELETE FROM image_embeddings WHERE id=?1", params![id])?;
                // Playback position is keyed by the video's own id.
                tx.execute(
                    "DELETE FROM video_play_progress WHERE media_id=?1",
                    params![id],
                )?;
            }
            tx.execute(
                "DELETE FROM media_item WHERE media_id=?1",
                params![rows.first().map(|(_, id)| id.clone())],
            )?;
            let removed = tx.execute("DELETE FROM media_files WHERE path=?1", params![path])?;
            tx.commit()?;
            Ok(removed)
        })
    }

    pub fn media_file_list(
        &self,
        data_type: DataType,
        limit: u32,
        offset: u32,
    ) -> rusqlite::Result<Vec<MediaFileRow>> {
        let data_type = kind(data_type)?;
        self.with_conn(|conn| {
            let mut statement = conn.prepare(&format!("SELECT {COLUMNS} FROM media_files WHERE data_type=?1 AND is_trashed=0 ORDER BY updated_at DESC,id LIMIT ?2 OFFSET ?3"))?;
            statement.query_map(params![data_type,limit,offset],from_row)?.collect()
        })
    }

    pub fn media_file_count(&self, data_type: DataType) -> rusqlite::Result<i64> {
        let data_type = kind(data_type)?;
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM media_files WHERE data_type=?1 AND is_trashed=0",
                [data_type],
                |row| row.get(0),
            )
        })
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/db/repositories/media_file.rs"]
mod tests;
