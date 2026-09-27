//! Content-addressable chat file store (port of `plain-app` `AppFileStore.kt`).
//!
//! Files uploaded as `isAppFile = true` (e.g. chat attachments) are
//! content-addressed by SHA-256 and stored in a sharded directory layout:
//!
//! ```text
//! {data_dir}/files/{hash[0..1]}/{hash[2..3]}/{hash}.{ext}
//! ```
//!
//! The `fid:` URI scheme (`fid:{hash}.{ext}`) embeds both the hash and the
//! extension so path resolution never needs a database query — the file
//! server can map the `fid` suffix straight to a path under the `files/`
//! root.
//!
//! Dedup uses a two-step check (mirrors `AppFileStore.importFile`):
//!
//! 1. **Weak probe** — `size` + SHA-256 of `head(4K) || tail(4K)`. Cheap
//!    index lookup via `app_files(size, weak_hash)` index.
//! 2. **Strong check** — full SHA-256. Only paid when the weak probe matches.
//!
//! On hit, the existing record is reused (and `ref_count` incremented). On
//! miss, the file is copied into the canonical location and a new
//! `app_files` row is inserted.

use std::fs;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::utils::hex::bytes_to_hex;
use crate::utils::mime::mime_extension;

use crate::chat::db::{ChatDb, DAppFile, DChat};

/// Default MIME type when the client did not supply one.
const DEFAULT_MIME: &str = "application/octet-stream";

/// Chunk size used by the weak hash — first 4 KB + last 4 KB of the file.
const WEAK_HEAD: usize = 4 * 1024;
const WEAK_TAIL: usize = 4 * 1024;

/// Result of importing a file into the store.
#[derive(Debug, Clone)]
pub struct ImportResult {
    /// SHA-256 hex digest (primary key in `app_files`).
    pub id: String,
    /// Final on-disk file name (`"{hash}.{ext}"`).
    pub fid_suffix: String,
    /// Effective MIME type used.
    pub mime_type: String,
    /// Final absolute path.
    pub real_path: PathBuf,
    /// `true` if an existing record was reused (dedup hit).
    pub reused: bool,
}

/// MIME → fid file extension. Returns an empty string for unknown types so
/// the `fid:` keeps no extension (the caller decides on `bin`). Delegates to
/// the shared `mime_extension` table (mirrors `plain-app`
/// `AppFileStore.extFromMime` / Android `MimeTypeMap`), mapping its `"bin"`
/// fallback to `""` for the app-file naming context.
fn fid_ext(mime_type: &str) -> &'static str {
    match mime_extension(mime_type) {
        "bin" => "",
        e => e,
    }
}

/// File-name extension (lowercased), falling back to the MIME-derived one.
/// The original file name is the ground truth for the extension: browsers
/// report an empty `File.type` for less-common extensions (`properties`,
/// `apk`, …), and the chunked-merge path has no MIME at all. Only when the
/// name carries no extension do we consult the MIME type.
fn ext_from_name(file_name: &str, mime_type: &str) -> String {
    Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .filter(|e| !e.is_empty())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_else(|| fid_ext(mime_type).to_string())
}

/// `fid_suffix` for a record whose canonical file already exists on disk —
/// derived from the stored `real_path` file name so a dedup hit returns the
/// exact suffix the original import produced (which may differ from what
/// `fid_ext(record.mime_type)` would recompute).
fn fid_suffix_of(real_path: &Path, strong_hash: &str) -> String {
    real_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(strong_hash)
        .to_string()
}

/// Derive the canonical destination path for a `{hash, ext}` pair.
pub fn dest_path(data_dir: &Path, hash: &str, ext: &str) -> PathBuf {
    data_dir.join(relative_dest_path(hash, ext))
}

/// Relative portion of [dest_path] — `files/{aa}/{bb}/{name}` — stored in
/// the `app_files.real_path` column to avoid repeating the platform-
/// specific `data_dir` prefix on every row.
pub fn relative_dest_path(hash: &str, ext: &str) -> String {
    let name = if ext.is_empty() {
        hash.to_string()
    } else {
        format!("{hash}.{ext}")
    };
    if hash.len() < 4 {
        return format!("files/{name}");
    }
    format!("files/{}/{}/{}", &hash[..2], &hash[2..4], name)
}

/// Compute the strong (full-file) SHA-256 hex digest.
fn strong_hash_file(path: &Path) -> std::io::Result<String> {
    let mut f = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(bytes_to_hex(&hasher.finalize()))
}

/// Compute the weak (head 4K + tail 4K) SHA-256 hex digest.
///
/// Files smaller than `WEAK_HEAD + WEAK_TAIL` are hashed in full; the head
/// and tail ranges still get covered (with overlap) and the result is
/// deterministic.
fn weak_hash_file(path: &Path) -> std::io::Result<(String, u64)> {
    let metadata = fs::metadata(path)?;
    let size = metadata.len();
    let mut f = fs::File::open(path)?;
    let mut hasher = Sha256::new();

    if size as usize <= WEAK_HEAD + WEAK_TAIL {
        // Whole file fits — hash everything (the head/tail windows collapse).
        let mut buf = vec![0u8; size as usize];
        f.read_exact(&mut buf)?;
        hasher.update(&buf);
    } else {
        let mut head = vec![0u8; WEAK_HEAD];
        f.read_exact(&mut head)?;
        hasher.update(&head);
        f.seek(std::io::SeekFrom::End(-(WEAK_TAIL as i64)))?;
        let mut tail = vec![0u8; WEAK_TAIL];
        f.read_exact(&mut tail)?;
        hasher.update(&tail);
    }

    Ok((bytes_to_hex(&hasher.finalize()), size))
}

/// `importFile` equivalent. The caller is responsible for deleting `src` if
/// `delete_src = true` is desired — we always copy (the source may still be
/// needed by the caller for retry / atomicity reasons).
///
/// `file_name` is the original upload file name — its extension is the
/// primary source for the on-disk extension. `mime_type` is the
/// client-supplied type used as a fallback when the name has no extension;
/// an empty value falls back to `application/octet-stream`.
pub fn import_file(
    db: &ChatDb,
    data_dir: &Path,
    src: &Path,
    file_name: &str,
    mime_type: &str,
) -> std::io::Result<ImportResult> {
    let (weak_hash, size) = weak_hash_file(src)?;
    let strong_hash = strong_hash_file(src)?;

    // Step 1: weak probe.
    let weak_candidates = db.find_app_files_by_weak(size as i64, &weak_hash);
    for cand in weak_candidates {
        if cand.id == strong_hash {
            // Step 2: strong match — reuse. The record's `real_path` is the
            // authority for where the canonical file lives (its extension
            // may have come from the original upload's file name).
            db.increment_app_file_ref(&strong_hash);
            let real_path = data_dir.join(&cand.real_path);
            ensure_canonical_exists(&real_path, src)?;
            let fid_suffix = fid_suffix_of(&real_path, &strong_hash);
            return Ok(ImportResult {
                id: strong_hash,
                fid_suffix,
                mime_type: cand.mime_type,
                real_path,
                reused: true,
            });
        }
    }

    // Step 2 (race guard): direct id lookup.
    if let Some(existing) = db.get_app_file(&strong_hash) {
        db.increment_app_file_ref(&strong_hash);
        let real_path = data_dir.join(&existing.real_path);
        ensure_canonical_exists(&real_path, src)?;
        let fid_suffix = fid_suffix_of(&real_path, &strong_hash);
        return Ok(ImportResult {
            id: strong_hash,
            fid_suffix,
            mime_type: existing.mime_type,
            real_path,
            reused: true,
        });
    }

    // No match — insert new record.
    let effective_mime = if mime_type.is_empty() {
        DEFAULT_MIME.to_string()
    } else {
        mime_type.to_string()
    };
    let ext = ext_from_name(file_name, &effective_mime);
    let real_path = dest_path(data_dir, &strong_hash, &ext);
    if let Some(parent) = real_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(src, &real_path)?;

    let now = crate::chat::db::now_iso();
    let record = DAppFile {
        id: strong_hash.clone(),
        size: size as i64,
        mime_type: effective_mime.clone(),
        real_path: relative_dest_path(&strong_hash, &ext),
        ref_count: 1,
        weak_hash,
        created_at: now.clone(),
        updated_at: now,
    };
    db.insert_app_file(&record);

    let fid_suffix = if ext.is_empty() {
        strong_hash.clone()
    } else {
        format!("{strong_hash}.{ext}")
    };
    Ok(ImportResult {
        id: strong_hash,
        fid_suffix,
        mime_type: effective_mime,
        real_path,
        reused: false,
    })
}

/// In-memory variant — useful for tests or small synthetic uploads.
pub fn import_bytes(
    db: &ChatDb,
    data_dir: &Path,
    data: &[u8],
    mime_type: &str,
) -> std::io::Result<ImportResult> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let strong_hash = bytes_to_hex(&hasher.finalize());

    if let Some(existing) = db.get_app_file(&strong_hash) {
        db.increment_app_file_ref(&strong_hash);
        let real_path = data_dir.join(&existing.real_path);
        let fid_suffix = fid_suffix_of(&real_path, &strong_hash);
        return Ok(ImportResult {
            id: strong_hash,
            fid_suffix,
            mime_type: existing.mime_type,
            real_path,
            reused: true,
        });
    }

    let effective_mime = if mime_type.is_empty() {
        DEFAULT_MIME.to_string()
    } else {
        mime_type.to_string()
    };
    let ext = fid_ext(&effective_mime);
    let real_path = dest_path(data_dir, &strong_hash, ext);
    if let Some(parent) = real_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&real_path, data)?;

    // Weak hash for in-memory data is the same as strong (we have the full
    // bytes). Use first 4K + last 4K to stay consistent with the file path
    // for index reuse.
    let mut weak = Sha256::new();
    if data.len() <= WEAK_HEAD + WEAK_TAIL {
        weak.update(data);
    } else {
        weak.update(&data[..WEAK_HEAD]);
        weak.update(&data[data.len() - WEAK_TAIL..]);
    }
    let weak_hash = bytes_to_hex(&weak.finalize());

    let now = crate::chat::db::now_iso();
    let record = DAppFile {
        id: strong_hash.clone(),
        size: data.len() as i64,
        mime_type: effective_mime.clone(),
        real_path: relative_dest_path(&strong_hash, ext),
        ref_count: 1,
        weak_hash,
        created_at: now.clone(),
        updated_at: now,
    };
    db.insert_app_file(&record);

    let fid_suffix = if ext.is_empty() {
        strong_hash.clone()
    } else {
        format!("{strong_hash}.{ext}")
    };
    Ok(ImportResult {
        id: strong_hash,
        fid_suffix,
        mime_type: effective_mime,
        real_path,
        reused: false,
    })
}

/// If the canonical path is missing but the source is a normal file, copy.
/// (The `app_files` table can outlive its backing file after a manual
/// wipe; `importFile` in plain-app silently restores it.)
fn ensure_canonical_exists(real_path: &Path, src: &Path) -> std::io::Result<()> {
    if real_path.exists() {
        return Ok(());
    }
    if let Some(parent) = real_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(src, real_path)?;
    Ok(())
}

/// Async helper: create a temp file in `dir` and return the path + handle.
/// The caller is expected to rename / delete the file as part of its own
/// atomicity strategy.
pub async fn write_temp_async(
    dir: &Path,
    prefix: &str,
    ext: &str,
) -> std::io::Result<(PathBuf, tokio::fs::File)> {
    tokio::fs::create_dir_all(dir).await?;
    let name = format!("{prefix}_{}.{}", std::process::id(), ext);
    let path = dir.join(name);
    let f = tokio::fs::File::create(&path).await?;
    Ok((path, f))
}

/// Stream an `AsyncRead` source to a `tokio::fs::File`, returning the final size.
pub async fn copy_to_async(
    dst: &mut tokio::fs::File,
    mut src: impl tokio::io::AsyncRead + Unpin,
) -> std::io::Result<u64> {
    let mut buf = vec![0u8; 64 * 1024];
    let mut total: u64 = 0;
    loop {
        let n = src.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        dst.write_all(&buf[..n]).await?;
        total += n as u64;
    }
    dst.flush().await?;
    Ok(total)
}

// ── Display names for app files (GraphQL AppFile.fileName) ─────────────────

use std::collections::HashMap;

/// Map content-hash prefix → original upload file name by scanning chat
/// contents (newest chat wins). The `fid:` URI prefix and any extension
/// are stripped so the key matches `DAppFile.id` (the bare hash).
pub fn file_name_map(chats: &[DChat]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut sorted: Vec<&DChat> = chats.iter().collect();
    sorted.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    for chat in sorted {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&chat.content) else {
            continue;
        };
        let Some(items) = v
            .get("value")
            .and_then(|vv| vv.get("items"))
            .and_then(|i| i.as_array())
        else {
            continue;
        };
        for item in items {
            let (Some(uri), Some(name)) = (
                item.get("uri").and_then(|u| u.as_str()),
                item.get("fileName").and_then(|n| n.as_str()),
            ) else {
                continue;
            };
            if !uri.starts_with("fid:") || name.is_empty() {
                continue;
            }
            let hash = uri.strip_prefix("fid:").unwrap_or(uri);
            let key = hash.split('.').next().unwrap_or(hash);
            map.entry(key.to_string())
                .or_insert_with(|| name.to_string());
        }
    }
    map
}

/// Display name for an app-file record: the original upload name when a
/// chat references it, else a generic `file.{ext}` from the MIME table.
pub fn display_name(file: &DAppFile, name_map: &HashMap<String, String>) -> String {
    let from_chat = name_map
        .get(&file.id)
        .cloned()
        .unwrap_or_default()
        .trim()
        .to_string();
    if !from_chat.is_empty() {
        return from_chat;
    }
    let ext = mime_extension(&file.mime_type);
    if ext == "bin" {
        "file".to_string()
    } else {
        format!("file.{ext}")
    }
}

#[cfg(test)]
#[path = "../../tests/unit/chat/app_file_store.rs"]
mod tests;
