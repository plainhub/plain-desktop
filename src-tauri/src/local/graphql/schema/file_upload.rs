//! File-upload GraphQL operations.
//!
//! Mirrors `plain-app` `web/schemas/FileUploadGraphQL.kt` — the three
//! operations the chunked uploader depends on:
//!
//! - `uploadedChunks(fileId)` — list already-uploaded chunk indices/sizes
//!   (used for resume; the client skips chunks that the server confirms
//!   it already has).
//! - `deleteChunks(fileId)` — clear the staging directory; called when
//!   all server chunks are stale (size mismatch).
//! - `mergeChunks(fileId, totalChunks, path, replace, isAppFile)` —
//!   assemble a final file from the staged chunks, then either import
//!   into the content-addressable store (when `isAppFile`) or write to
//!   the requested `path`.

use async_graphql::{Context, Error as GqlError, Object, Result as GqlResult};
use std::path::PathBuf;
use std::sync::Arc;

use crate::local::app_file_store;
use crate::local::graphql::context::AppCtx;


#[derive(Default)]
pub struct FileUploadQuery;

#[derive(Default)]
pub struct FileUploadMutation;

fn chunk_dir(ctx: &AppCtx, file_id: &str) -> PathBuf {
    ctx.data_dir.join("upload_tmp").join(file_id)
}

fn respond<T: Into<String>>(msg: T) -> GqlError {
    GqlError::new(msg.into())
}

#[Object]
impl FileUploadQuery {
    /// List the chunk indices the server already has on disk for `file_id`,
    /// in the format `"<index>:<size>"` (matches what the web client
    /// expects — see `lib/upload/upload.ts::getUploadedChunks`).
    async fn uploaded_chunks(&self, ctx: &Context<'_>, file_id: String) -> Vec<String> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let dir = chunk_dir(c, &file_id);
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => return vec![],
        };
        let mut out: Vec<(i32, u64)> = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            // Staging filenames are `chunk_<index>`; skip partial temp files
            // (`.tmp_chunk_*`).
            let Some(idx) = name.strip_prefix("chunk_") else {
                continue;
            };
            let Ok(idx) = idx.parse::<i32>() else {
                continue;
            };
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            out.push((idx, meta.len()));
        }
        out.sort_by_key(|(i, _)| *i);
        out.into_iter().map(|(i, s)| format!("{i}:{s}")).collect()
    }
}

#[Object]
impl FileUploadMutation {
    /// Recursively remove the staging directory for `file_id`.
    /// Idempotent — returns `true` whether the directory existed or not.
    async fn delete_chunks(&self, ctx: &Context<'_>, file_id: String) -> bool {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let dir = chunk_dir(c, &file_id);
        if dir.exists() {
            let _ = std::fs::remove_dir_all(&dir);
        }
        true
    }

    /// Assemble the chunked upload into a final file.
    ///
    /// - When `isAppFile` is `true`, the file is imported into the
    ///   content-addressable store (dedup) and the returned fidSuffix
    ///   (`"{hash}.{ext}"`) is what the client uses to build a `fid:` URI.
    /// - When `false`, the merged file is written to `path` (or a
    ///   non-conflicting sibling when `replace = false`); the return
    ///   value is the on-disk base name.
    ///
    /// Return format: `"{fidSuffix|fileName}:{mergedSize}"` — matches
    /// plain-app `mergeChunks` so the web client's response parser keeps
    /// working.
    async fn merge_chunks(
        &self,
        ctx: &Context<'_>,
        file_id: String,
        total_chunks: i32,
        path: String,
        replace: bool,
        is_app_file: bool,
    ) -> GqlResult<String> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>().clone();
        let dir = chunk_dir(&c, &file_id);
        if !dir.exists() {
            return Err(respond(format!("No chunks found for {file_id}")));
        }

        // Pre-flight: every chunk file must exist; compute expected size.
        let mut expected_size: u64 = 0;
        for i in 0..total_chunks {
            let chunk = dir.join(format!("chunk_{i}"));
            if !chunk.exists() {
                return Err(respond(format!("Missing chunk {i}")));
            }
            expected_size += std::fs::metadata(&chunk)
                .map_err(|e| respond(format!("chunk {i} stat: {e}")))?
                .len();
        }

        // Merge into a temp file first, then atomic rename.
        let temp_merge = dir.join(format!(".merge_tmp_{file_id}_{}", std::process::id()));
        let merge_result = merge_chunks_to(&dir, total_chunks, &temp_merge);
        if let Err(e) = merge_result {
            let _ = std::fs::remove_file(&temp_merge);
            return Err(e);
        }

        let merged_size = std::fs::metadata(&temp_merge)
            .map(|m| m.len())
            .unwrap_or(0);
        if merged_size != expected_size {
            let _ = std::fs::remove_file(&temp_merge);
            return Err(respond(format!(
                "Merge integrity failed: expected {expected_size}, got {merged_size}"
            )));
        }

        if is_app_file {
            // Import into the content-addressable store. The merged temp
            // file is the source; `import_file` moves (copies) it into the
            // canonical location and inserts/updates the `app_files` row.
            // The temp file is left for the caller to clean up; we delete
            // it eagerly here to mirror plain-app behaviour (which uses
            // `deleteSrc = true` and renames within `importFile`). The
            // original file name rides in `path` — its extension decides
            // the on-disk extension (the multipart chunk parts carry no
            // usable MIME for less-common types).
            let file_name = std::path::Path::new(&path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let result = app_file_store::import_file(&c.db, &c.data_dir, &temp_merge, &file_name, "")
                .map_err(|e| respond(format!("import failed: {e}")))?;
            let _ = std::fs::remove_dir_all(&dir);
            let _ = std::fs::remove_file(&temp_merge);
            Ok(format!("{}:{}", result.fid_suffix, merged_size))
        } else {
            let target = PathBuf::from(&path);
            if let Some(parent) = target.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            let final_path = if replace {
                if target.exists() {
                    let _ = std::fs::remove_file(&target);
                }
                target.clone()
            } else if target.exists() {
                plain_rs::utils::unique_path::unique_sibling(&target)
            } else {
                target.clone()
            };

            // Atomic rename; fall back to copy on cross-device / permission issues.
            if std::fs::rename(&temp_merge, &final_path).is_err() {
                std::fs::copy(&temp_merge, &final_path)
                    .map_err(|e| respond(format!("save merged file: {e}")))?;
                let _ = std::fs::remove_file(&temp_merge);
            }

            let _ = std::fs::remove_dir_all(&dir);

            let final_name = final_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            Ok(format!("{final_name}:{merged_size}"))
        }
    }
}

fn merge_chunks_to(
    dir: &std::path::Path,
    total_chunks: i32,
    out: &std::path::Path,
) -> GqlResult<()> {
    use std::fs::File;
    use std::io::{Read, Write};
    let mut out_f = File::create(out).map_err(|e| respond(format!("merge create: {e}")))?;
    let mut buf = [0u8; 64 * 1024];
    for i in 0..total_chunks {
        let mut chunk = File::open(dir.join(format!("chunk_{i}")))
            .map_err(|e| respond(format!("chunk {i} open: {e}")))?;
        loop {
            let n = chunk
                .read(&mut buf)
                .map_err(|e| respond(format!("chunk {i} read: {e}")))?;
            if n == 0 {
                break;
            }
            out_f
                .write_all(&buf[..n])
                .map_err(|e| respond(format!("merge write: {e}")))?;
        }
    }
    out_f.flush().map_err(|e| respond(format!("merge flush: {e}")))?;
    Ok(())
}
