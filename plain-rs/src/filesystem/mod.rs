use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};
mod rename;
pub mod tasks;
pub mod writes;
pub mod browse;
pub mod record;
pub type TransferProgress = dyn Fn(i64, i64) -> std::io::Result<()> + Send + Sync;

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
    file_info_to_model(p, &meta, meta.is_dir())
}

/// Sort order for directory listing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SortBy {
    #[serde(alias = "TAKEN_AT_DESC")]
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
) -> std::io::Result<Vec<FileEntry>> {
    let limit = if limit == 0 { 1000 } else { limit };
    let entries = tokio::fs::read_dir(dir).await?;

    // Collect DirEntry items (lightweight — no extra stat).
    struct Entry {
        name: String,
        is_dir: bool,
        meta: std::fs::Metadata,
    }

    let mut list: Vec<Entry> = Vec::new();
    let mut entries = std::pin::pin!(entries);
    while let Some(e) = entries.next_entry().await? {
        let name = e.file_name().to_string_lossy().to_string();
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        // Use the metadata already cached by the OS for read_dir entries.
        let meta = e.metadata().await?;
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
        return Ok(Vec::new());
    }
    let end = offset.saturating_add(limit).min(list.len());
    let page = &list[offset..end];

    // Build FileEntry only for the page.
    let mut out = Vec::with_capacity(page.len());
    for e in page {
        let full = dir.join(&e.name);
        out.push(file_info_to_model(&full, &e.meta, e.is_dir)?);
    }
    Ok(out)
}

/// Mirrors Go `ListFiles`: reads a single directory level and returns
/// all entries (no pagination). Used when sorting requires stat on all
/// entries (DATE_*, SIZE_* sorts with small dirs).
pub async fn list_dir(dir: &Path, show_hidden: bool) -> std::io::Result<Vec<FileEntry>> {
    let entries = tokio::fs::read_dir(dir).await?;
    let mut out = Vec::new();
    let mut entries = std::pin::pin!(entries);
    while let Some(e) = entries.next_entry().await? {
        let name = e.file_name().to_string_lossy().to_string();
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        let meta = e.metadata().await?;
        let full = dir.join(&name);
        out.push(file_info_to_model(&full, &meta, meta.is_dir())?);
    }
    Ok(out)
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
            let n = read_dirents(std::os::fd::AsRawFd::as_raw_fd(&fd), &mut buf)?;
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
        Ok(count)
    }
}

/// Mirrors Go `renameFileModel` (`internal/graph/files_dir_rename_api.go`).
/// Fails if the destination already exists. We do **not** touch the media
/// index here — callers that need it call `media_scan::delete_by_path` +
/// `media_scan::scan` themselves, the same way the Go handlers do.
pub async fn rename(src: &Path, dst: &Path) -> std::io::Result<()> {
    let from = src.to_path_buf();
    let to = dst.to_path_buf();
    let result = tokio::task::spawn_blocking(move || rename::no_replace(&from, &to))
        .await
        .map_err(std::io::Error::other)?;
    match result {
        Err(error) if rename::copy_required(&error, false) => {
            copy_and_remove(src, dst, false).await
        }
        result => result,
    }
}

/// Mirrors Go `copyFileOp` (`internal/graph/files_copy_move_api.go`).
///
/// Resolution rules (preserved verbatim from the Go side):
/// - If `src == dst`, copy to a sibling of `src` named after the original.
/// - If `dst` is an existing directory, copy `src` into `dst/base(src)`.
/// - If `!overwrite` and the resolved `dst` exists, pick a unique
///   `name_N.ext` instead of erroring.
/// - Refuses to copy a directory into itself.
pub async fn copy_path(src: &Path, dst: &Path, overwrite: bool) -> std::io::Result<PathBuf> {
    copy_path_with_progress(src, dst, overwrite, None).await
}
pub async fn copy_path_with_progress(
    src: &Path,
    dst: &Path,
    overwrite: bool,
    progress: Option<&TransferProgress>,
) -> std::io::Result<PathBuf> {
    let src = clean(src);
    let info = tokio::fs::symlink_metadata(&src).await?;
    let resolved = resolve_destination(&src, dst, overwrite).await?;
    require_separate_paths(&src, &resolved, info.is_dir()).await?;
    copy_to(&src, &resolved, overwrite, progress).await?;
    Ok(resolved)
}

pub async fn move_path(src: &Path, dst: &Path, overwrite: bool) -> std::io::Result<PathBuf> {
    move_path_with_progress(src, dst, overwrite, None).await
}
pub async fn move_path_with_progress(
    src: &Path,
    dst: &Path,
    overwrite: bool,
    progress: Option<&TransferProgress>,
) -> std::io::Result<PathBuf> {
    let src = clean(src);
    let info = tokio::fs::symlink_metadata(&src).await?;
    let resolved = resolve_destination(&src, dst, overwrite).await?;
    require_separate_paths(&src, &resolved, info.is_dir()).await?;
    let totals = if progress.is_some() {
        let path = src.clone();
        Some(
            tokio::task::spawn_blocking(move || measure(&path))
                .await
                .map_err(std::io::Error::other)??,
        )
    } else {
        None
    };
    let result = if overwrite {
        tokio::fs::rename(&src, &resolved).await
    } else {
        let from = src.clone();
        let to = resolved.clone();
        tokio::task::spawn_blocking(move || rename::no_replace(&from, &to))
            .await
            .map_err(std::io::Error::other)?
    };
    match result {
        Ok(()) => {
            if let (Some(callback), Some((bytes, items))) = (progress, totals) {
                callback(bytes, items)?;
            }
            Ok(resolved)
        }
        Err(error) if rename::copy_required(&error, overwrite) => {
            copy_to(&src, &resolved, overwrite, progress).await?;
            remove(&src).await?;
            Ok(resolved)
        }
        Err(error) => Err(error),
    }
}

async fn copy_and_remove(src: &Path, dst: &Path, overwrite: bool) -> std::io::Result<()> {
    copy_to(src, dst, overwrite, None).await?;
    remove(src).await
}

async fn resolve_destination(src: &Path, dst: &Path, overwrite: bool) -> std::io::Result<PathBuf> {
    let mut resolved = clean(dst);
    if src.as_os_str().is_empty() || resolved.as_os_str().is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid arguments",
        ));
    }
    if resolved != src {
        match tokio::fs::metadata(&resolved).await {
            Ok(info) if info.is_dir() => resolved.push(
                src.file_name()
                    .ok_or_else(|| std::io::Error::other("source has no name"))?,
            ),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    if !overwrite || resolved == src {
        let target = resolved.clone();
        resolved =
            tokio::task::spawn_blocking(move || crate::utils::unique_path::unique_sibling(&target))
                .await
                .map_err(std::io::Error::other)??;
    }
    Ok(resolved)
}

async fn require_separate_paths(src: &Path, dst: &Path, is_dir: bool) -> std::io::Result<()> {
    let source = match tokio::fs::canonicalize(src).await {
        Ok(path) => path,
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                && tokio::fs::symlink_metadata(src)
                    .await?
                    .file_type()
                    .is_symlink() =>
        {
            let parent = src
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            tokio::fs::canonicalize(parent).await?.join(
                src.file_name()
                    .ok_or_else(|| std::io::Error::other("invalid source"))?,
            )
        }
        Err(error) => return Err(error),
    };
    let mut ancestor = dst.to_path_buf();
    let mut suffix = Vec::new();
    let destination = loop {
        match tokio::fs::canonicalize(&ancestor).await {
            Ok(mut canonical) => {
                for segment in suffix.iter().rev() {
                    canonical.push(segment);
                }
                break canonical;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                suffix.push(
                    ancestor
                        .file_name()
                        .ok_or_else(|| std::io::Error::other("invalid destination"))?
                        .to_os_string(),
                );
                if !ancestor.pop() || ancestor.as_os_str().is_empty() {
                    ancestor = std::env::current_dir()?;
                }
            }
            Err(error) => return Err(error),
        }
    };
    let aliases_source = if tokio::fs::try_exists(&destination).await? {
        let from = source.clone();
        let to = destination.clone();
        tokio::task::spawn_blocking(move || rename::same_identity(&from, &to))
            .await
            .map_err(std::io::Error::other)??
    } else {
        false
    };
    if destination == source || aliases_source || (is_dir && destination.starts_with(&source)) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "cannot copy or move into source",
        ));
    }
    Ok(())
}

async fn copy_to(
    src: &Path,
    dst: &Path,
    overwrite: bool,
    progress: Option<&TransferProgress>,
) -> std::io::Result<()> {
    let info = tokio::fs::symlink_metadata(src).await?;
    if info.is_dir() {
        copy_dir_recursive(src, dst, overwrite, progress).await
    } else if info.file_type().is_symlink() {
        copy_link(src, dst, overwrite).await?;
        if let Some(callback) = progress {
            callback(i64::try_from(info.len()).map_err(std::io::Error::other)?, 1)?;
        }
        Ok(())
    } else {
        copy_file_contents(src, dst, overwrite, progress).await
    }
}

async fn copy_link(src: &Path, dst: &Path, overwrite: bool) -> std::io::Result<()> {
    let target = tokio::fs::read_link(src).await?;
    if overwrite {
        match tokio::fs::symlink_metadata(dst).await {
            Ok(_) => tokio::fs::remove_file(dst).await?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    #[cfg(unix)]
    {
        tokio::fs::symlink(&target, dst).await
    }
    #[cfg(windows)]
    {
        if tokio::fs::metadata(src).await?.is_dir() {
            tokio::fs::symlink_dir(&target, dst).await
        } else {
            tokio::fs::symlink_file(&target, dst).await
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
    if p.is_absolute() && p.parent().is_none() {
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
        let canonical = tokio::fs::canonicalize(&p).await?;
        if canonical.parent().is_none() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "refusing to delete root",
            ));
        }
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

fn file_info_to_model(
    path: &Path,
    info: &std::fs::Metadata,
    is_dir: bool,
) -> std::io::Result<FileEntry> {
    let child_count = if is_dir {
        count_dir_entries_fast(path, DIR_CHILDREN_THRESHOLD)?
    } else {
        0
    };
    let updated_at: DateTime<Utc> = info.modified()?.into();
    Ok(FileEntry {
        path: path
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/"),
        is_dir,
        created_at: updated_at,
        updated_at,
        size: i64::try_from(info.len()).map_err(|_| std::io::Error::other("file size overflow"))?,
        child_count,
    })
}

async fn copy_file_contents(
    src: &Path,
    dst: &Path,
    overwrite: bool,
    progress: Option<&TransferProgress>,
) -> std::io::Result<()> {
    if let Some(parent) = dst.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await?;
        }
    }
    if !tokio::fs::symlink_metadata(src).await?.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "unsupported file type",
        ));
    }
    let mut in_f = tokio::fs::File::open(src).await?;
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true);
    if overwrite {
        options.create(true).truncate(true);
    } else {
        options.create_new(true);
    }
    let mut out_f = options.open(dst).await?;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let length = in_f.read(&mut buffer).await?;
        if length == 0 {
            break;
        }
        out_f.write_all(&buffer[..length]).await?;
        if let Some(callback) = progress {
            callback(length as i64, 0)?;
        }
    }
    out_f
        .set_permissions(in_f.metadata().await?.permissions())
        .await?;
    out_f.sync_all().await?;
    if let Some(callback) = progress {
        callback(0, 1)?;
    }
    Ok(())
}

async fn copy_dir_recursive(
    src: &Path,
    dst: &Path,
    overwrite: bool,
    progress: Option<&TransferProgress>,
) -> std::io::Result<()> {
    if let Some(parent) = dst.parent().filter(|p| !p.as_os_str().is_empty()) {
        tokio::fs::create_dir_all(parent).await?;
    }
    if overwrite {
        tokio::fs::create_dir_all(dst).await?;
    } else {
        tokio::fs::create_dir(dst).await?;
    }
    let mut stack: Vec<(PathBuf, PathBuf)> = vec![(src.to_path_buf(), dst.to_path_buf())];
    while let Some((s, d)) = stack.pop() {
        let mut rd = tokio::fs::read_dir(&s).await?;
        while let Some(entry) = rd.next_entry().await? {
            let ft = entry.file_type().await?;
            let child_src = entry.path();
            let child_dst = d.join(entry.file_name());
            if ft.is_dir() {
                if overwrite {
                    tokio::fs::create_dir_all(&child_dst).await?;
                } else {
                    tokio::fs::create_dir(&child_dst).await?;
                }
                stack.push((child_src, child_dst));
            } else if ft.is_symlink() {
                copy_link(&child_src, &child_dst, overwrite).await?;
                if let Some(callback) = progress {
                    callback(
                        i64::try_from(entry.metadata().await?.len())
                            .map_err(std::io::Error::other)?,
                        1,
                    )?;
                }
            } else {
                copy_file_contents(&child_src, &child_dst, overwrite, progress).await?;
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
pub(crate) fn count_dir_entries_fast(path: &Path, threshold: i32) -> std::io::Result<i32> {
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
            let n = read_dirents(std::os::fd::AsRawFd::as_raw_fd(&fd), &mut buf)?;
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
        Ok(count)
    }
}

#[cfg(target_os = "linux")]
fn open_dir(path: &Path) -> std::io::Result<std::os::fd::OwnedFd> {
    // O_RDONLY | O_DIRECTORY on Linux. The constant 0o200000 is
    // O_DIRECTORY on glibc; we set it via libc::O_DIRECTORY.
    let flags = libc::O_RDONLY | 0o200_000;
    let cstr = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())?;
    let fd = unsafe { libc::open(cstr.as_ptr(), flags) };
    if fd < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(unsafe { std::os::fd::FromRawFd::from_raw_fd(fd) })
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

#[cfg(test)]
#[path = "../../tests/unit/filesystem.rs"]
mod tests;

pub fn measure(path: &Path) -> std::io::Result<(i64, i64)> {
    let mut directories = Vec::<std::fs::ReadDir>::new();
    let mut next = Some(path.to_path_buf());
    let mut bytes = 0_i64;
    let mut items = 0_i64;
    loop {
        let path = if let Some(path) = next.take() {
            path
        } else {
            loop {
                let Some(directory) = directories.last_mut() else {
                    return Ok((bytes, items));
                };
                if let Some(entry) = directory.next() {
                    break entry?.path();
                }
                directories.pop();
            }
        };
        let info = std::fs::symlink_metadata(&path)?;
        if info.is_dir() {
            directories.push(std::fs::read_dir(path)?);
        } else if info.is_file() || info.file_type().is_symlink() {
            bytes = bytes
                .checked_add(i64::try_from(info.len()).map_err(std::io::Error::other)?)
                .ok_or_else(|| std::io::Error::other("file size overflow"))?;
            items = items
                .checked_add(1)
                .ok_or_else(|| std::io::Error::other("file count overflow"))?;
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "unsupported file type",
            ));
        }
    }
}
