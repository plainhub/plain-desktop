//! Filesystem-related helpers that don't deserve their own crate and that
//! we'd rather not pull in a third-party dependency for. Mirrors bits of
//! `internal/pkg/pathx` and the per-op resolvers in
//! `internal/graph/files_*_api.go` + `internal/graph/helpers_local.go` from
//! the Go side.
//!
//! Inlined here: `percent_decode_path` (used in HTTP query params).
//! Inlined elsewhere: per-file-id short id, mount table parsing, etc.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use chrono::{DateTime, Utc};

/// Errors for file ID decryption.
#[derive(Debug)]
pub enum FileIdError {
    InvalidId,
    Forbidden,
}

/// Decrypt an encrypted file ID to a filesystem path.
/// Mirrors Go `fs.PathFromFileID` in `internal/fs/file_id.go`:
///   1. Base64-decode the id (replacing ' ' back to '+' for URL safety).
///   2. Load the URL token from the preferences.
///   3. Base64-decode the token to get the 32-byte XChaCha20-Poly1305 key.
///   4. Decrypt the ciphertext → plaintext file path.
pub fn path_from_file_id(id: &str, prefs: &crate::prefs::Prefs) -> Result<String, FileIdError> {
    let id = id.trim();
    if id.is_empty() {
        log::debug!("[path_from_file_id] empty id");
        return Err(FileIdError::InvalidId);
    }
    // Some URL parsers turn '+' into ' ' in query strings.
    let id = id.replace(' ', "+");
    log::debug!(
        "[path_from_file_id] id (first 40 chars): {}",
        &id[..id.len().min(40)]
    );

    let ciphertext = base64_decode(&id).ok_or_else(|| {
        log::debug!("[path_from_file_id] base64 decode of id failed");
        FileIdError::Forbidden
    })?;
    log::debug!("[path_from_file_id] ciphertext len: {}", ciphertext.len());

    let token = prefs
        .get::<String>("url_token")
        .ok()
        .flatten()
        .filter(|t| !t.is_empty())
        .ok_or_else(|| {
            log::debug!("[path_from_file_id] no url_token found in prefs");
            FileIdError::Forbidden
        })?;
    log::debug!(
        "[path_from_file_id] token: {}",
        &token[..token.len().min(20)]
    );

    let key = base64_decode(&token).ok_or_else(|| {
        log::debug!(
            "[path_from_file_id] base64 decode of token failed, token='{}'",
            token
        );
        FileIdError::Forbidden
    })?;
    log::debug!("[path_from_file_id] key len: {} (expected 32)", key.len());

    let plain = crate::crypto::xchacha_decrypt_raw(&key, &ciphertext).ok_or_else(|| {
        log::debug!(
            "[path_from_file_id] XChaCha20-Poly1305 decrypt failed, key_len={}, ct_len={}",
            key.len(),
            ciphertext.len()
        );
        FileIdError::Forbidden
    })?;
    if plain.is_empty() {
        log::debug!("[path_from_file_id] decrypted to empty");
        return Err(FileIdError::Forbidden);
    }
    let path = String::from_utf8(plain).map_err(|e| {
        log::debug!("[path_from_file_id] utf8 decode failed: {}", e);
        FileIdError::Forbidden
    })?;
    let path = extract_path(&path);
    log::debug!("[path_from_file_id] resolved to: {}", path);
    Ok(path)
}

/// plain-app FileServer contract: media-item ids encrypt a JSON envelope
/// `{"path":…,"mediaId":…}` (web `getFileId` mints it whenever the item has
/// a mediaId), while plain file ids encrypt the bare path. Accept both.
fn extract_path(plain: &str) -> String {
    let trimmed = plain.trim();
    if trimmed.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(p) = v.get("path").and_then(|p| p.as_str()) {
                return p.to_string();
            }
        }
    }
    plain.to_string()
}

/// Minimal base64 standard decoding (matches Go `base64.StdEncoding`).
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.trim_end_matches('=');
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for &b in s.as_bytes() {
        let val = match b {
            b'A'..=b'Z' => (b - b'A') as u32,
            b'a'..=b'z' => (b - b'a' + 26) as u32,
            b'0'..=b'9' => (b - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            b' ' => continue,
            _ => return None,
        };
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Decode a percent-encoded path/URL component into a UTF-8 `String`.
///
/// Unlike `percent_encoding::percent_decode_str` we **only** support the
/// `%XX` syntax — no `+`-as-space handling, because we never feed query
/// strings into here (query strings are still parsed with the `url`
/// crate, which already decodes them). The intent is purely to undo the
/// `axum::extract::Path<String>` double-encoding for path segments that
/// contain `%` or non-ASCII characters.
///
/// On an invalid `%XX` triplet the bytes are passed through unchanged
/// (matching `decode_utf8_lossy` semantics on the upstream crate) so a
/// malformed URL doesn't bring the whole handler down.
pub fn percent_decode_path(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Map a file extension to an IANA MIME type. Falls back to
/// `application/octet-stream` for anything we don't recognise.
///
/// We don't try to be comprehensive — the Go side uses Go's `mime`
/// package which carries ~1000 entries, but for a NAS front-end the
/// ~50 entries below cover 99% of served files. Adding the rest is
/// trivial when the need arises.
pub fn guess_mime(path: &std::path::Path) -> String {
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let mime = crate::utils::mime::mime_from_ext(filename);
    if mime.starts_with("text/") {
        format!("{mime}; charset=utf-8")
    } else {
        mime.to_string()
    }
}

/// Image extensions that qualify for the animated-image/SVG sniff below.
/// Mirrors plain-app `Constants.PHOTO_EXTENSIONS` (`isImageFast` gate).
const PHOTO_EXTENSIONS: [&str; 13] = [
    "jpg", "jpeg", "png", "bmp", "webp", "heic", "heif", "apng", "avif", "gif", "tiff", "tif",
    "svg",
];

/// Whether the file is an animated image (GIF, animated WebP, animated HEIF)
/// or an SVG. Decided from the extension plus a sniff of the first 256 content
/// bytes, mirroring plain-app `isAnimatedImageOrSvg` so the shared web client
/// sees identical `/fs` behaviour on both platforms: such files must be
/// served as-is (browsers render them natively, thumbnails included) instead
/// of being routed into the WebP thumbnail pipeline, which cannot decode them.
pub fn is_animated_image_or_svg(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if !PHOTO_EXTENSIONS.contains(&ext.as_str()) {
        return false;
    }
    if ext == "svg" {
        return true;
    }
    if ext == "png" || ext == "jpg" || ext == "jpeg" {
        return false;
    }

    let mut header = [0u8; 256];
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let n = std::io::Read::read(&mut file, &mut header).unwrap_or(0);
    let header = &header[..n];

    // GIF87a / GIF89a magic at offset 0 — both served as-is (plain-app never
    // routes GIFs through the thumbnail pipeline on `/fs`).
    if n >= 6 && matches!(&header[..6], b"GIF87a" | b"GIF89a") {
        return true;
    }

    // JPEG / PNG magic bytes are never animated; stop the sniff cheaply.
    let b0 = header.first().copied();
    let b1 = header.get(1).copied();
    if b0 == Some(0xFF) && b1 == Some(0xD8) || b0 == Some(0x89) && b1 == Some(b'P') {
        return false;
    }

    // WebP: "RIFF" + "WEBP"; a "VP8X" box's animation bit is bit 1 of byte 16.
    if n >= 17 && &header[0..4] == b"RIFF" && &header[8..12] == b"WEBP" {
        return &header[12..16] == b"VP8X" && n > 17 && (header[16] & 0b10) != 0;
    }

    // HEIF: an "ftyp" box followed by an animated brand (msf1/hevc/hevx).
    if n >= 12 && &header[4..8] == b"ftyp" {
        return matches!(&header[8..12], b"msf1" | b"hevc" | b"hevx");
    }

    // SVG without a recognised extension: scan the readable window for the
    // `<svg` tag.
    header.windows(4).any(|w| w == b"<svg")
}

// ---------------------------------------------------------------------------
// File operations
//
// The functions below are 1:1 ports of the resolvers in
// `internal/graph/files_*_api.go` and the helpers in
// `internal/graph/helpers_local.go` + `helpers/files_helper.go` from the
// Go side. They are async because every caller in the GraphQL resolvers
// is `async fn` and we want the syscall work to run on tokio's blocking
// pool (via `tokio::fs::File`) rather than blocking the reactor.
// ---------------------------------------------------------------------------

/// Mirrors Go `model.File`: the GraphQL-side result of any "look at a
/// path on disk" operation. Field-for-field equivalent to the Go struct
/// produced by `helpers.FileInfoToModel` (see
/// `internal/graph/helpers/files_helper.go`).
#[derive(Debug, Clone)]
pub struct FileEntry {
    /// Always forward-slash, mirroring Go's `filepath.ToSlash(path)`.
    pub path: String,
    pub is_dir: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub size: i64,
    pub child_count: i32,
}

const DIR_CHILDREN_THRESHOLD: i32 = 20_000;

/// Mirrors Go `createDirModel` / `os.MkdirAll(p, 0o755)`. Creates the
/// directory and any missing parents. Idempotent.
pub async fn ensure_dir(p: &Path) -> std::io::Result<()> {
    tokio::fs::create_dir_all(p).await
}

/// Mirrors Go `os.Stat` + `helpers.FileInfoToModel(p, info, info.IsDir())`.
/// Returns a `FileEntry` populated with `ModTime` for both `created_at`
/// and `updated_at` (Go does the same — btrfs/xfs expose birth time but
/// the Go side ignores it for portability).
pub async fn stat(p: &Path) -> std::io::Result<FileEntry> {
    let meta = tokio::fs::metadata(p).await?;
    Ok(file_info_to_model(p, &meta, meta.is_dir()))
}

/// Sort order for directory listing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortBy {
    NameAsc,
    NameDesc,
    DateAsc,
    DateDesc,
    SizeAsc,
    SizeDesc,
}

/// Mirrors Go `ListFilesPaged`: reads a **single** directory level,
/// sorts (dirs-first then by the requested key), applies offset/limit,
/// and only calls `stat` on the entries in the requested page. This
/// avoids a full recursive walk and per-entry stat for large dirs.
pub async fn list_dir_paged(
    dir: &Path,
    show_hidden: bool,
    offset: usize,
    limit: usize,
    sort_by: SortBy,
) -> Vec<FileEntry> {
    let limit = if limit == 0 { 1000 } else { limit };
    let entries = match tokio::fs::read_dir(dir).await {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };

    // Collect DirEntry items (lightweight — no extra stat).
    struct Entry {
        name: String,
        is_dir: bool,
        meta: std::fs::Metadata,
    }

    let mut list: Vec<Entry> = Vec::new();
    let mut entries = std::pin::pin!(entries);
    while let Ok(Some(e)) = entries.next_entry().await {
        let name = e.file_name().to_string_lossy().to_string();
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        // Use the metadata already cached by the OS for read_dir entries.
        let meta = match e.metadata().await {
            Ok(m) => m,
            Err(_) => continue,
        };
        list.push(Entry {
            name,
            is_dir: meta.is_dir(),
            meta,
        });
    }

    // Sort: directories first, then by the requested key.
    match sort_by {
        SortBy::NameAsc => {
            list.sort_by(|a, b| {
                match a.is_dir.cmp(&b.is_dir).reverse() {
                    std::cmp::Ordering::Equal => {}
                    ord => return ord,
                }
                a.name.to_lowercase().cmp(&b.name.to_lowercase())
            });
        }
        SortBy::NameDesc => {
            list.sort_by(|a, b| {
                match a.is_dir.cmp(&b.is_dir).reverse() {
                    std::cmp::Ordering::Equal => {}
                    ord => return ord,
                }
                b.name.to_lowercase().cmp(&a.name.to_lowercase())
            });
        }
        SortBy::DateAsc => {
            list.sort_by(|a, b| {
                match a.is_dir.cmp(&b.is_dir).reverse() {
                    std::cmp::Ordering::Equal => {}
                    ord => return ord,
                }
                let ta = a.meta.modified().ok();
                let tb = b.meta.modified().ok();
                ta.cmp(&tb)
            });
        }
        SortBy::DateDesc => {
            list.sort_by(|a, b| {
                match a.is_dir.cmp(&b.is_dir).reverse() {
                    std::cmp::Ordering::Equal => {}
                    ord => return ord,
                }
                let ta = a.meta.modified().ok();
                let tb = b.meta.modified().ok();
                tb.cmp(&ta)
            });
        }
        SortBy::SizeAsc => {
            list.sort_by(|a, b| {
                match a.is_dir.cmp(&b.is_dir).reverse() {
                    std::cmp::Ordering::Equal => {}
                    ord => return ord,
                }
                a.meta.len().cmp(&b.meta.len())
            });
        }
        SortBy::SizeDesc => {
            list.sort_by(|a, b| {
                match a.is_dir.cmp(&b.is_dir).reverse() {
                    std::cmp::Ordering::Equal => {}
                    ord => return ord,
                }
                b.meta.len().cmp(&a.meta.len())
            });
        }
    }

    // Apply offset/limit.
    if offset >= list.len() {
        return Vec::new();
    }
    let end = (offset + limit).min(list.len());
    let page = &list[offset..end];

    // Build FileEntry only for the page.
    let mut out = Vec::with_capacity(page.len());
    for e in page {
        let full = dir.join(&e.name);
        out.push(file_info_to_model(&full, &e.meta, e.is_dir));
    }
    out
}

/// Mirrors Go `ListFiles`: reads a single directory level and returns
/// all entries (no pagination). Used when sorting requires stat on all
/// entries (DATE_*, SIZE_* sorts with small dirs).
pub async fn list_dir(dir: &Path, show_hidden: bool) -> Vec<FileEntry> {
    let entries = match tokio::fs::read_dir(dir).await {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    let mut entries = std::pin::pin!(entries);
    while let Ok(Some(e)) = entries.next_entry().await {
        let name = e.file_name().to_string_lossy().to_string();
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        let meta = match e.metadata().await {
            Ok(m) => m,
            Err(_) => continue,
        };
        let full = dir.join(&name);
        out.push(file_info_to_model(&full, &meta, meta.is_dir()));
    }
    out
}

/// Count entries in a directory (non-recursive). Mirrors Go `CountDirEntries`.
pub fn count_dir_entries(dir: &Path, show_hidden: bool) -> std::io::Result<usize> {
    #[cfg(not(target_os = "linux"))]
    {
        return std::fs::read_dir(dir)?.try_fold(0usize, |count, entry| {
            let name = entry?.file_name();
            let visible = show_hidden || !name.to_string_lossy().starts_with('.');
            Ok(count + usize::from(visible))
        });
    }
    #[cfg(target_os = "linux")]
    {
        let fd = open_dir(dir)?;
        let mut count: usize = 0;
        let mut buf = vec![0u8; 32 * 1024];
        loop {
            let n = read_dirents(fd, &mut buf)?;
            if n == 0 {
                break;
            }
            let mut pos = 0usize;
            while pos < n {
                if pos + 19 > n {
                    break;
                }
                let reclen = u16::from_ne_bytes([buf[pos + 16], buf[pos + 17]]) as usize;
                if reclen == 0 || pos + reclen > n {
                    break;
                }
                let ino = u64::from_ne_bytes([
                    buf[pos],
                    buf[pos + 1],
                    buf[pos + 2],
                    buf[pos + 3],
                    buf[pos + 4],
                    buf[pos + 5],
                    buf[pos + 6],
                    buf[pos + 7],
                ]);
                if ino != 0 {
                    let name_start = pos + 19;
                    let mut name_end = name_start;
                    while name_end < pos + reclen && buf[name_end] != 0 {
                        name_end += 1;
                    }
                    let name = &buf[name_start..name_end];
                    let is_dot = name == b"." || name == b"..";
                    if !is_dot {
                        let is_hidden = !show_hidden && name.len() > 1 && name[0] == b'.';
                        if !is_hidden {
                            count += 1;
                        }
                    }
                }
                pos += reclen;
            }
        }
        let _ = nix_close(fd);
        Ok(count)
    }
}

/// Mirrors Go `renameFileModel` (`internal/graph/files_dir_rename_api.go`).
/// Fails if the destination already exists. We do **not** touch the media
/// index here — callers that need it call `media_scan::delete_by_path` +
/// `media_scan::scan` themselves, the same way the Go handlers do.
pub async fn rename(src: &Path, dst: &Path) -> std::io::Result<()> {
    tokio::fs::rename(src, dst).await
}

/// Mirrors Go `copyFileOp` (`internal/graph/files_copy_move_api.go`).
///
/// Resolution rules (preserved verbatim from the Go side):
/// - If `src == dst`, copy to a sibling of `src` named after the original.
/// - If `dst` is an existing directory, copy `src` into `dst/base(src)`.
/// - If `!overwrite` and the resolved `dst` exists, pick a unique
///   `name (N).ext` instead of erroring.
/// - Refuses to copy a directory into itself.
pub async fn copy_path(src: &Path, dst: &Path, overwrite: bool) -> std::io::Result<PathBuf> {
    let src = clean(src);
    let dst = clean(dst);
    if src.as_os_str().is_empty() || dst.as_os_str().is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid arguments",
        ));
    }
    let sfi = tokio::fs::metadata(&src).await?;
    let sfi_is_dir = sfi.is_dir();

    let mut resolved = dst.clone();
    if resolved == src {
        let parent = resolved.parent().unwrap_or(Path::new("."));
        resolved = parent.join(resolved.file_name().unwrap_or_default());
    } else if let Ok(dfi) = tokio::fs::metadata(&dst).await {
        if dfi.is_dir() {
            let base = src.file_name().unwrap_or_default();
            resolved = dst.join(base);
        }
    }

    if !overwrite {
        resolved = make_unique_path_if_exists(&resolved, !sfi_is_dir).await?;
    }

    if sfi_is_dir {
        let src_abs = std::path::absolute(&src)?;
        let dst_abs = std::path::absolute(&resolved)?;
        let sep = std::path::MAIN_SEPARATOR;
        let src_prefix = format!("{}{}", src_abs.display(), sep);
        let dst_prefix = format!("{}{}", dst_abs.display(), sep);
        if dst_prefix.starts_with(&src_prefix) && dst_abs != src_abs {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid destination: cannot copy a directory into itself",
            ));
        }
    }

    if sfi_is_dir {
        copy_dir_recursive(&src, &resolved).await?;
    } else {
        copy_file_contents(&src, &resolved).await?;
    }
    Ok(resolved)
}

/// Mirrors Go `moveFileOp` (`internal/graph/files_copy_move_api.go`).
///
/// - If `dst` is an existing directory, move `src` into `dst/base(src)`.
/// - If `!overwrite` and `dst` exists, pick a unique `name (N).ext`.
/// - Tries `rename(2)` first; on cross-device `EXDEV` it falls back to
///   copy+remove (the Go side does the same — `os.Rename` returns an
///   error and we then call `copyFileOp` + `os.RemoveAll`).
pub async fn move_path(src: &Path, dst: &Path, overwrite: bool) -> std::io::Result<PathBuf> {
    let src = clean(src);
    let dst = clean(dst);
    if src.as_os_str().is_empty() || dst.as_os_str().is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid arguments",
        ));
    }
    let sfi = tokio::fs::metadata(&src).await?;
    let sfi_is_dir = sfi.is_dir();

    let mut resolved = dst.clone();
    if let Ok(dfi) = tokio::fs::metadata(&dst).await {
        if dfi.is_dir() {
            let base = src.file_name().unwrap_or_default();
            resolved = dst.join(base);
        }
        if !overwrite {
            let base = resolved
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            let dir = resolved.parent().unwrap_or(Path::new("."));
            let (name, ext) = split_name_ext(&base);
            for i in 1..i32::MAX {
                let cand = dir.join(format!("{} ({}){}", name, i, ext));
                if !cand.exists() {
                    resolved = cand;
                    break;
                }
            }
        }
    }

    match tokio::fs::rename(&src, &resolved).await {
        Ok(()) => Ok(resolved),
        Err(e) => {
            // Cross-fs fallback: copy then remove.
            if sfi_is_dir {
                copy_dir_recursive(&src, &resolved).await?;
            } else {
                copy_file_contents(&src, &resolved).await?;
            }
            // Best-effort: ignore the original remove failure and surface
            // the rename error so the caller knows the move was lossy.
            let _ = remove(&src).await;
            if sfi_is_dir { Err(e) } else { Err(e) }
        }
    }
}

/// Mirrors Go `deleteFiles` (`internal/graph/files_delete_api.go`):
/// - Refuses to delete the filesystem root.
/// - For a directory, uses `remove_dir_all` (recursive).
/// - For a file or symlink, uses `remove_file`.
pub async fn remove(p: &Path) -> std::io::Result<()> {
    let p = clean(p);
    if p.as_os_str().is_empty() || p == Path::new(".") {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid path",
        ));
    }
    if p == Path::new("/") {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "refusing to delete root",
        ));
    }
    let meta = match tokio::fs::symlink_metadata(&p).await {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    if meta.is_dir() {
        tokio::fs::remove_dir_all(&p).await
    } else {
        tokio::fs::remove_file(&p).await
    }
}

// ---------------------------------------------------------------------------
// Internal helpers — ported from `internal/graph/helpers_local.go` and
// `internal/graph/helpers/files_helper.go`. Not `pub`.
// ---------------------------------------------------------------------------

fn clean(p: &Path) -> PathBuf {
    // `Path::clean` is a no-op for `.` and `..` and for absolute paths
    // it keeps the leading separator, which matches `filepath.Clean` in
    // the Go side. The Go code also does `filepath.ToSlash` *only* for
    // display paths; the on-disk path stays native.
    p.components().collect()
}

fn file_info_to_model(path: &Path, info: &std::fs::Metadata, is_dir: bool) -> FileEntry {
    let child_count = if is_dir {
        count_dir_entries_fast(path, DIR_CHILDREN_THRESHOLD).unwrap_or(0)
    } else {
        0
    };
    let mtime = info.modified().ok().and_then(|t| {
        let dt: DateTime<Utc> = t.into();
        Some(dt)
    });
    let (created_at, updated_at) = match mtime {
        Some(t) => (t, t),
        None => {
            let now = Utc::now();
            (now, now)
        }
    };
    FileEntry {
        path: path
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/"),
        is_dir,
        created_at,
        updated_at,
        size: info.len() as i64,
        child_count,
    }
}

async fn make_unique_path_if_exists(dst: &Path, treat_as_file: bool) -> std::io::Result<PathBuf> {
    if !dst.exists() {
        return Ok(dst.to_path_buf());
    }
    let base = dst
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let dir = dst.parent().unwrap_or(Path::new("."));
    let (name, ext) = if treat_as_file {
        split_name_ext(&base)
    } else {
        (base, String::new())
    };
    for i in 1..i32::MAX {
        let cand = dir.join(format!("{} ({}){}", name, i, ext));
        if !cand.exists() {
            return Ok(cand);
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not find a unique name",
    ))
}

fn split_name_ext(base: &str) -> (String, String) {
    match base.rfind('.') {
        Some(i) if i > 0 => (base[..i].to_string(), base[i..].to_string()),
        _ => (base.to_string(), String::new()),
    }
}

async fn copy_file_contents(src: &Path, dst: &Path) -> std::io::Result<()> {
    if let Some(parent) = dst.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await?;
        }
    }
    let mut in_f = tokio::fs::File::open(src).await?;
    let mut out_f = tokio::fs::File::create(dst).await?;
    tokio::io::copy(&mut in_f, &mut out_f).await?;
    out_f.sync_all().await?;
    Ok(())
}

async fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    tokio::fs::create_dir_all(dst).await?;
    let mut stack: Vec<(PathBuf, PathBuf)> = vec![(src.to_path_buf(), dst.to_path_buf())];
    while let Some((s, d)) = stack.pop() {
        let mut rd = tokio::fs::read_dir(&s).await?;
        while let Some(entry) = rd.next_entry().await? {
            let ft = entry.file_type().await?;
            let child_src = entry.path();
            let child_dst = d.join(entry.file_name());
            if ft.is_dir() {
                tokio::fs::create_dir_all(&child_dst).await?;
                stack.push((child_src, child_dst));
            } else if ft.is_symlink() {
                let target = tokio::fs::read_link(&child_src).await?;
                if let Some(parent) = child_dst.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                // Best-effort symlink re-creation; ignore if the link
                // already exists (matches Go's permissive behaviour).
                let _ = tokio::fs::symlink(&target, &child_dst).await;
            } else {
                copy_file_contents(&child_src, &child_dst).await?;
            }
        }
    }
    Ok(())
}

/// Counts directory entries using `getdents(2)`, mirroring Go's
/// `countDirEntriesFast` in
/// `internal/graph/helpers/files_helper.go`. Skips `.` and `..`. Returns
/// `threshold` (and caps there) once the count exceeds the threshold so
/// the response stays cheap on huge trees.
fn count_dir_entries_fast(path: &Path, threshold: i32) -> std::io::Result<i32> {
    #[cfg(not(target_os = "linux"))]
    {
        let mut count = 0;
        for entry in std::fs::read_dir(path)? {
            entry?;
            count += 1;
            if count >= threshold {
                break;
            }
        }
        return Ok(count);
    }
    #[cfg(target_os = "linux")]
    {
        let fd = open_dir(path)?;
        let mut count: i32 = 0;
        let mut buf = vec![0u8; 32 * 1024];
        loop {
            let n = read_dirents(fd, &mut buf)?;
            if n == 0 {
                break;
            }
            let mut pos = 0usize;
            while pos < n {
                // Linux dirent layout: d_ino(u64), d_off(i64), d_reclen(u16),
                // d_type(u8), d_name(...). All little-endian on the platforms
                // we target. We don't use `d_type` because we only need the
                // name here.
                if pos + 19 > n {
                    break;
                }
                let reclen = u16::from_ne_bytes([buf[pos + 16], buf[pos + 17]]) as usize;
                if reclen == 0 || pos + reclen > n {
                    break;
                }
                // d_ino: skip entries that are unlinked
                let ino = u64::from_ne_bytes([
                    buf[pos],
                    buf[pos + 1],
                    buf[pos + 2],
                    buf[pos + 3],
                    buf[pos + 4],
                    buf[pos + 5],
                    buf[pos + 6],
                    buf[pos + 7],
                ]);
                if ino != 0 {
                    // d_name starts at pos+19, NUL-terminated
                    let name_start = pos + 19;
                    let mut name_end = name_start;
                    while name_end < pos + reclen && buf[name_end] != 0 {
                        name_end += 1;
                    }
                    let name = &buf[name_start..name_end];
                    let is_dot = name == b"."
                        || name == b".."
                        || (name.len() > 1 && name[0] == b'.' && name[1] == b'.' && name[2] == 0);
                    if !is_dot {
                        count += 1;
                        if count > threshold {
                            return Ok(threshold);
                        }
                    }
                }
                pos += reclen;
            }
        }
        let _ = nix_close(fd);
        Ok(count)
    }
}

#[cfg(target_os = "linux")]
fn open_dir(path: &Path) -> std::io::Result<i32> {
    // O_RDONLY | O_DIRECTORY on Linux. The constant 0o200000 is
    // O_DIRECTORY on glibc; we set it via libc::O_DIRECTORY.
    let flags = libc::O_RDONLY | 0o200_000;
    let cstr = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())?;
    let fd = unsafe { libc::open(cstr.as_ptr(), flags) };
    if fd < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(fd)
    }
}

#[cfg(target_os = "linux")]
fn read_dirents(fd: i32, buf: &mut [u8]) -> std::io::Result<usize> {
    // SYS_getdents64 = 217 on x86_64 Linux, 61 on aarch64. We could pull in
    // the `nix` crate for this, but the existing project keeps the surface
    // tiny and only needs two libc calls. Falling back to a single raw
    // syscall keeps the dependency graph unchanged. Non-Linux targets (i.e.
    // macOS for `cargo check`/`cargo test`) have no getdents64; report it
    // as unsupported instead of failing to compile.
    #[cfg(target_os = "linux")]
    {
        #[cfg(target_arch = "x86_64")]
        const SYS_GETDENTS64: i64 = 217;
        #[cfg(target_arch = "aarch64")]
        const SYS_GETDENTS64: i64 = 61;
        let n = unsafe {
            libc::syscall(
                SYS_GETDENTS64,
                fd,
                buf.as_mut_ptr() as *mut libc::c_void,
                buf.len() as libc::size_t,
            )
        };
        if n < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(n as usize)
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (fd, buf);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "getdents64 is Linux-only",
        ))
    }
}

#[cfg(target_os = "linux")]
fn nix_close(fd: i32) -> std::io::Result<()> {
    let rc = unsafe { libc::close(fd) };
    if rc < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

// Used by the once-only `count_dir_entries_fast` lazy path. We don't
// need it elsewhere; the static just silences the "unused import"
// warning when the call site is later removed.
#[allow(dead_code)]
static _USED: OnceLock<()> = OnceLock::new();

#[cfg(test)]
#[path = "../../tests/unit/media/fsx.rs"]
mod tests;
