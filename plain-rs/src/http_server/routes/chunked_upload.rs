//! Chunked upload storage and merge.
//!
//! 1:1 port of `internal/graph/uploaded_chunks_api.go` +
//! `internal/graph/upload_merge_chunks_api.go`.
//!
//! Layout: `<DATA_DIR>/upload_tmp/<fileID>/chunk_<index>`. The frontend POSTs
//! each chunk to `/upload_chunk` with form fields `fileID`, `index`, `file`.
//! When all chunks arrive, the frontend invokes the GraphQL `mergeChunks`
//! mutation, which concatenates `chunk_0..chunk_total-1` into the final
//! destination. The chunk directory is removed after a successful merge.
//!
//! Media scanning is delegated to a no-op `trigger_media_scan` (TODO 8.8).

use anyhow::Result;
use std::path::{Path, PathBuf};

const UPLOAD_SUBDIR: &str = "upload_tmp";
pub async fn save_chunk(data_dir: &Path, file_id: &str, index: i32, bytes: &[u8]) -> Result<PathBuf> {
    crate::uploads::save_chunk(&data_dir.join(UPLOAD_SUBDIR),file_id,i64::from(index),bytes).await
}
pub async fn list_uploaded_chunks(data_dir: &Path, file_id: &str) -> Result<Vec<i32>> {
    Ok(crate::uploads::chunks(&data_dir.join(UPLOAD_SUBDIR),file_id).await?.into_iter().filter_map(|(index,_)|i32::try_from(index).ok()).collect())
}

/// Concatenate chunks into `path`.
///
/// - `replace=false`: if `path` already exists, append a `(N)` suffix before
///   the file extension, exactly as the Go side does.
/// - On success the chunk directory is removed.
/// - `trigger_media_scan` is called at the end (currently a no-op).
pub async fn merge_chunks(
    data_dir: &Path,
    file_id: &str,
    total_chunks: i32,
    path: &str,
    replace: bool,
) -> Result<(String, u64)> {
    let dir = crate::uploads::chunk_dir(&data_dir.join(UPLOAD_SUBDIR), file_id)?;
    let mut expected = 0u64;
    for index in 0..total_chunks {
        expected += tokio::fs::metadata(dir.join(format!("chunk_{index}")))
            .await?
            .len();
    }
    crate::uploads::merge(
        None,
        &dir,
        total_chunks,
        expected as i64,
        crate::uploads::Kind::File {
            path: path.into(),
            replace,
        },
    )
    .await
}

/// Trigger a media scan for the freshly merged file. No-op stub; will be
/// wired up when 8.8 media scanner lands.
pub async fn trigger_media_scan(_path: &Path) {
    // TODO(8.8): media::ScanFile(path)
}

#[cfg(test)]
#[path = "../../../tests/unit/api/chunked_upload.rs"]
mod tests;
