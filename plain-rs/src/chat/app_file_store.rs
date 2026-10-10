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

mod attachment_commit;
pub mod chat_deletion;
pub(crate) mod content_refs;
pub use content_refs::{collect as content_reference_ids, release_unbound};

use std::fs;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::utils::hex::bytes_to_hex;
use crate::utils::mime::mime_extension;

use crate::db::{DAppFile, DChat, Db};

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
    pub chat: Option<DChat>,
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
    db: &Db,
    data_dir: &Path,
    src: &Path,
    file_name: &str,
    mime_type: &str,
) -> std::io::Result<ImportResult> {
    let guessed_mime = crate::utils::mime::mime_from_ext(if file_name.is_empty() {
        src.to_str().unwrap_or_default()
    } else {
        file_name
    });
    let mime_type = if mime_type.is_empty() {
        guessed_mime
    } else {
        mime_type
    };
    let strong = strong_hash_file(src)?;
    let (weak, size) = weak_hash_file(src)?;
    store(
        db,
        data_dir,
        &strong,
        &weak,
        size,
        file_name,
        mime_type,
        None,
        |path| {
            fs::copy(src, path)?;
            if strong_hash_file(path)? != strong || fs::metadata(path)?.len() != size {
                return Err(std::io::Error::other("source changed during import"));
            }
            Ok(())
        },
    )
}

pub fn import_bytes(
    db: &Db,
    data_dir: &Path,
    data: &[u8],
    mime_type: &str,
) -> std::io::Result<ImportResult> {
    let strong = bytes_to_hex(&Sha256::digest(data));
    let mut weak = Sha256::new();
    if data.len() <= WEAK_HEAD + WEAK_TAIL {
        weak.update(data);
    } else {
        weak.update(&data[..WEAK_HEAD]);
        weak.update(&data[data.len() - WEAK_TAIL..]);
    }
    store(
        db,
        data_dir,
        &strong,
        &bytes_to_hex(&weak.finalize()),
        data.len() as u64,
        "",
        mime_type,
        None,
        |path| fs::write(path, data),
    )
}

fn store(
    db: &Db,
    directory: &Path,
    strong: &str,
    weak: &str,
    size: u64,
    name: &str,
    mime: &str,
    attachment: Option<attachment_commit::Selection<'_>>,
    write: impl FnOnce(&Path) -> std::io::Result<()>,
) -> std::io::Result<ImportResult> {
    let size: i64 = size.try_into().map_err(std::io::Error::other)?;
    let _guard = db.app_files_lock()?;
    let existing = db.app_file_get(strong).map_err(std::io::Error::other)?;
    let effective_mime = existing
        .as_ref()
        .map(|file| file.mime_type.as_str())
        .unwrap_or(if mime.is_empty() { DEFAULT_MIME } else { mime });
    let relative = existing
        .as_ref()
        .map(|file| file.real_path.clone())
        .unwrap_or_else(|| relative_dest_path(strong, &ext_from_name(name, effective_mime)));
    let path = owned_path(directory, &relative)?;
    let mut installed = false;
    if !path.is_file() {
        let temporary = path.with_file_name(format!(
            ".import-{}",
            crate::utils::short_uuid::short_uuid()
        ));
        let result = write(&temporary).and_then(|_| {
            fs::File::open(&temporary)?.sync_all()?;
            fs::rename(&temporary, &path)
        });
        if let Err(error) = result {
            let _ = fs::remove_file(temporary);
            return Err(error);
        }
        installed = true;
    }
    let result = attachment_commit::commit(
        db,
        &DAppFile {
            id: strong.into(),
            size,
            mime_type: effective_mime.into(),
            real_path: relative,
            ref_count: 1,
            weak_hash: weak.into(),
            created_at: crate::db::now_iso(),
            updated_at: crate::db::now_iso(),
        },
        existing.is_some(),
        &fid_suffix_of(&path, strong),
        attachment,
    );
    if let Err(error) = result {
        if installed && existing.is_none() {
            let _ = fs::remove_file(&path);
        }
        return Err(std::io::Error::other(error));
    }
    Ok(ImportResult {
        id: strong.into(),
        fid_suffix: fid_suffix_of(&path, strong),
        mime_type: effective_mime.into(),
        real_path: path,
        reused: existing.is_some(),
        chat: result.unwrap(),
    })
}

fn owned_path(directory: &Path, relative: &str) -> std::io::Result<PathBuf> {
    let relative = Path::new(relative);
    if !relative.starts_with("files")
        || relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(std::io::Error::other("invalid app file path"));
    }
    let path = directory.join(relative);
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("invalid app file parent"))?;
    fs::create_dir_all(directory)?;
    let root = fs::canonicalize(directory)?;
    let mut ancestor = parent;
    while !ancestor.exists() {
        ancestor = ancestor
            .parent()
            .ok_or_else(|| std::io::Error::other("invalid app file parent"))?;
    }
    if !fs::canonicalize(ancestor)?.starts_with(&root) {
        return Err(std::io::Error::other("app file parent outside store"));
    }
    fs::create_dir_all(parent)?;
    if !fs::canonicalize(parent)?.starts_with(&root)
        || (path.exists() && !fs::canonicalize(&path)?.starts_with(&root))
    {
        return Err(std::io::Error::other("app file path outside store"));
    }
    Ok(path)
}

pub fn release(db: &Db, directory: &Path, id: &str) -> std::io::Result<bool> {
    let _guard = db.app_files_lock()?;
    let Some(file) = db.app_file_get(id).map_err(std::io::Error::other)? else {
        return Ok(false);
    };
    let path = owned_path(directory, &file.real_path)?;
    let quarantine = path.with_file_name(format!(
        ".release-{}",
        crate::utils::short_uuid::short_uuid()
    ));
    let moved = file.ref_count <= 1 && path.exists();
    if moved {
        fs::rename(&path, &quarantine)?;
    }
    let result = db.with_conn(|conn| {
        let tx = conn.unchecked_transaction()?;
        if file.ref_count > 1 {
            tx.execute(
                "UPDATE app_files SET ref_count=ref_count-1,updated_at=?1 WHERE id=?2",
                rusqlite::params![crate::db::now_iso(), id],
            )?;
        } else {
            tx.execute("DELETE FROM app_files WHERE id=?1", rusqlite::params![id])?;
        }
        tx.commit()
    });
    if let Err(error) = result {
        if moved {
            fs::rename(&quarantine, &path)?;
        }
        return Err(std::io::Error::other(error));
    }
    if moved {
        fs::remove_file(quarantine)?;
    }
    Ok(true)
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

pub fn import_attachment(
    db: &Db,
    data_dir: &Path,
    src: &Path,
    file_name: &str,
    message_id: &str,
    id: &str,
    original_uri: &str,
) -> std::io::Result<ImportResult> {
    let strong = strong_hash_file(src)?;
    let (weak, size) = weak_hash_file(src)?;
    store(
        db,
        data_dir,
        &strong,
        &weak,
        size,
        file_name,
        crate::utils::mime::mime_from_ext(file_name),
        Some(attachment_commit::Selection::Attachment {
            message_id,
            id,
            original_uri,
        }),
        |path| {
            fs::copy(src, path)?;
            if strong_hash_file(path)? != strong || fs::metadata(path)?.len() != size {
                return Err(std::io::Error::other("source changed during import"));
            }
            Ok(())
        },
    )
}

pub fn import_preview_image(
    db: &Db,
    directory: &Path,
    bytes: &[u8],
    mime: &str,
    message_id: &str,
    text: &str,
    preview: &serde_json::Value,
) -> std::io::Result<ImportResult> {
    let strong = bytes_to_hex(&Sha256::digest(bytes));
    let mut weak = Sha256::new();
    if bytes.len() <= WEAK_HEAD + WEAK_TAIL {
        weak.update(bytes)
    } else {
        weak.update(&bytes[..WEAK_HEAD]);
        weak.update(&bytes[bytes.len() - WEAK_TAIL..]);
    }
    store(
        db,
        directory,
        &strong,
        &bytes_to_hex(&weak.finalize()),
        bytes.len() as u64,
        "",
        mime,
        Some(attachment_commit::Selection::LinkPreview {
            message_id,
            text,
            preview,
        }),
        |path| fs::write(path, bytes),
    )
}
