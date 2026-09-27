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

use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

const CHUNK_PREFIX: &str = "chunk_";
const UPLOAD_SUBDIR: &str = "upload_tmp";

/// Path to the chunk directory for a given file ID.
pub fn chunk_dir(data_dir: &Path, file_id: &str) -> PathBuf {
    if file_id.is_empty() {
        // Defensive: an empty file_id would put chunks at the upload_tmp root.
        // We return a non-existent path so subsequent operations fail safely.
        return data_dir.join(UPLOAD_SUBDIR).join("_empty");
    }
    data_dir.join(UPLOAD_SUBDIR).join(file_id)
}

/// Path to a single chunk file.
#[allow(dead_code)]
pub fn chunk_path(data_dir: &Path, file_id: &str, index: i32) -> PathBuf {
    chunk_dir(data_dir, file_id).join(format!("{}{}", CHUNK_PREFIX, index))
}

/// Persist a chunk. `index` is validated; `file_id` must be non-empty.
#[allow(dead_code)]
pub async fn save_chunk(
    data_dir: &Path,
    file_id: &str,
    index: i32,
    bytes: &[u8],
) -> Result<PathBuf> {
    if file_id.trim().is_empty() {
        return Err(anyhow!("fileID must be non-empty"));
    }
    if index < 0 {
        return Err(anyhow!("chunk index must be >= 0"));
    }
    let dir = chunk_dir(data_dir, file_id);
    tokio::fs::create_dir_all(&dir).await?;
    let p = chunk_path(data_dir, file_id, index);
    let mut f = tokio::fs::File::create(&p).await?;
    f.write_all(bytes).await?;
    f.flush().await?;
    Ok(p)
}

/// List uploaded chunk indices for a `file_id`, sorted ascending. Mirrors
/// `uploadedChunks` in the Go side; an empty list is returned for unknown
/// `file_id` (no error).
pub async fn list_uploaded_chunks(data_dir: &Path, file_id: &str) -> Result<Vec<i32>> {
    let dir = chunk_dir(data_dir, file_id);
    let mut rd = match tokio::fs::read_dir(&dir).await {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut out: Vec<i32> = Vec::new();
    while let Some(entry) = rd.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(idx_str) = name.strip_prefix(CHUNK_PREFIX) {
            if let Ok(v) = idx_str.parse::<i32>() {
                out.push(v);
            }
        }
    }
    out.sort_unstable();
    Ok(out)
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
    if file_id.trim().is_empty() || path.trim().is_empty() || total_chunks <= 0 {
        return Err(anyhow!("invalid arguments"));
    }

    let base = chunk_dir(data_dir, file_id);

    // Pick a destination path
    let dest: PathBuf = {
        let cleaned = PathBuf::from(path);
        if replace {
            cleaned
        } else {
            match tokio::fs::metadata(&cleaned).await {
                Err(_) => cleaned,
                Ok(_) => unique_path(&cleaned).await,
            }
        }
    };

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await.ok();
    }
    let mut out = tokio::fs::File::create(&dest)
        .await
        .map_err(|e| anyhow!("create {}: {e}", dest.display()))?;

    for i in 0..total_chunks {
        let cp = base.join(format!("{}{}", CHUNK_PREFIX, i));
        let bytes = tokio::fs::read(&cp)
            .await
            .map_err(|_| anyhow!("missing chunk {}", i))?;
        tokio::io::AsyncWriteExt::write_all(&mut out, &bytes).await?;
    }
    out.flush().await?;
    drop(out);

    // Cleanup
    let _ = tokio::fs::remove_dir_all(&base).await;

    // Media scan (no-op until 8.8)
    trigger_media_scan(&dest).await;

    let merged_size = tokio::fs::metadata(&dest)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    Ok((
        dest.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string(),
        merged_size,
    ))
}

/// Append ` (N)` before the file extension, like the Go side. Searches for
/// the first available ` (1)`, ` (2)`, ...
async fn unique_path(dest: &Path) -> PathBuf {
    let parent = dest.parent().unwrap_or_else(|| Path::new("."));
    let base_name = dest
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string();
    let (stem, ext) = match base_name.rfind('.') {
        Some(pos) if pos > 0 => (&base_name[..pos], &base_name[pos..]),
        _ => (base_name.as_str(), ""),
    };
    for i in 1..i32::MAX {
        let cand = parent.join(format!("{} ({}){}", stem, i, ext));
        if tokio::fs::metadata(&cand).await.is_err() {
            return cand;
        }
    }
    dest.to_path_buf()
}

/// Trigger a media scan for the freshly merged file. No-op stub; will be
/// wired up when 8.8 media scanner lands.
pub async fn trigger_media_scan(_path: &Path) {
    // TODO(8.8): media::ScanFile(path)
}

#[cfg(test)]
#[path = "../../tests/unit/nas/chunked_upload.rs"]
mod tests;
