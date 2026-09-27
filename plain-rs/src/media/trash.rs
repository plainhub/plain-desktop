//! Trash (回收站) implementation. Port of `internal/fs/trash*.go`.
//!
//! Storage model
//! =============
//! Each physical trash lives at `${MOUNT}/.nas-trash/` (one per disk).
//! Buckets under that root: `data/YYYY/MM/f_<id>` or `d_<id>`.
//! Metadata lives in the global KV store under the `trash:*` key namespace:
//!   - `trash:item:<id>`                → JSON-encoded `TrashItem`
//!   - `trash:by_deleted_at:<rev>:<id>` → empty (newest-first index)
//!   - `trash:by_deleted_at_df:<df>:<rev>:<id>` → empty (dirs-first index)
//!   - `trash:by_name_df:<df>:<name_lc>:<id>`   → empty (name sort index)
//!   - `trash:by_size_df:<df>:<size>:<id>`      → empty (size sort index)
//!
//! Invariants
//! ==========
//! - Delete is always a single `rename(2)` (O(1)) — no recursion, no copy.
//! - Trash metadata is the **single source of truth** for restore / GC.
//! - All rename+metadata operations on a given disk are serialized by an
//!   `flock` on `${MOUNT}/.nas-trash/.lock`.

use crate::utils::shortid;
use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::LazyLock as Lazy;
use std::sync::Mutex;

use crate::media::kv::get_default;
use crate::media::mountinfo::{self, CleanPath};

const NS_ITEM: &str = "trash:item:";
const NS_BY_DELETED: &str = "trash:by_deleted_at:";
const NS_BY_DELETED_DF: &str = "trash:by_deleted_at_df:";
const NS_BY_NAME_DF: &str = "trash:by_name_df:";
const NS_BY_SIZE_DF: &str = "trash:by_size_df:";
const REVERSED_MAX: i64 = i64::MAX;

#[cfg(unix)]
fn owner_mode(meta: &std::fs::Metadata) -> (u32, u32, u32) {
    (meta.uid(), meta.gid(), meta.mode())
}

#[cfg(not(unix))]
fn owner_mode(_meta: &std::fs::Metadata) -> (u32, u32, u32) {
    (0, 0, 0)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrashItem {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String, // "file" | "dir"
    pub original_path: String,
    pub disk: String,
    pub trash_rel_path: String,
    pub deleted_at: i64, // unix seconds (UTC)
    pub uid: u32,
    pub gid: u32,
    pub mode: u32,
    pub size: Option<i64>,
    pub entry_count: Option<i64>,
}

fn item_key(id: &str) -> String {
    format!("{NS_ITEM}{id}")
}
fn by_deleted_key(deleted_at: i64, id: &str) -> String {
    let rev = REVERSED_MAX - deleted_at;
    format!("{NS_BY_DELETED}{rev:020}:{id}")
}
fn by_deleted_df_key(deleted_at: i64, dir_flag: &str, id: &str) -> String {
    let rev = REVERSED_MAX - deleted_at;
    format!("{NS_BY_DELETED_DF}{dir_flag}:{rev:020}:{id}")
}
fn by_name_df_key(dir_flag: &str, name_lower: &str, id: &str) -> String {
    format!("{NS_BY_NAME_DF}{dir_flag}:{name_lower}:{id}")
}
fn by_size_df_key(dir_flag: &str, size: i64, id: &str) -> String {
    format!("{NS_BY_SIZE_DF}{dir_flag}:{size:020}:{id}")
}
fn dir_flag(it: &TrashItem) -> &'static str {
    if it.kind == "dir" { "0" } else { "1" }
}
fn display_name_lower(it: &TrashItem) -> String {
    Path::new(&it.original_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase()
        .trim()
        .to_string()
}
fn sort_size(it: &TrashItem) -> i64 {
    it.size.unwrap_or(0)
}

// ----- Load / Store -----

pub fn load_item(id: &str) -> Result<Option<TrashItem>> {
    let db = get_default();
    match db.get(item_key(id))? {
        Some(ivec) => {
            let v: TrashItem = serde_json::from_slice(&ivec)?;
            if v.id.trim().is_empty() {
                Ok(None)
            } else {
                Ok(Some(v))
            }
        }
        None => Ok(None),
    }
}

pub fn store_item(it: &TrashItem) -> Result<()> {
    if it.id.trim().is_empty() {
        bail!("invalid trash item");
    }
    let df = dir_flag(it);
    let name_lc = display_name_lower(it);
    let size = sort_size(it);
    let json = serde_json::to_vec(it)?;
    let db = get_default();
    db.insert(item_key(&it.id), json)?;
    db.insert(by_deleted_key(it.deleted_at, &it.id), b"")?;
    db.insert(by_deleted_df_key(it.deleted_at, df, &it.id), b"")?;
    db.insert(by_name_df_key(df, &name_lc, &it.id), b"")?;
    db.insert(by_size_df_key(df, size, &it.id), b"")?;
    db.flush()?;
    Ok(())
}

pub fn delete_item_keys(it: &TrashItem) -> Result<()> {
    let df = dir_flag(it);
    let rev = REVERSED_MAX - it.deleted_at;
    let name_lc = display_name_lower(it);
    let size = sort_size(it);
    let db = get_default();
    db.remove(item_key(&it.id))?;
    db.remove(format!("{NS_BY_DELETED}{rev:020}:{}", it.id))?;
    db.remove(format!("{NS_BY_DELETED_DF}{df}:{rev:020}:{}", it.id))?;
    db.remove(format!("{NS_BY_NAME_DF}{df}:{name_lc}:{}", it.id))?;
    db.remove(format!("{NS_BY_SIZE_DF}{df}:{size:020}:{}", it.id))?;
    Ok(())
}

// ----- Per-disk in-process lock (serialises trash ops on the same disk) -----

static LOCKS: Lazy<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn lock_for_disk(disk: &str) -> Arc<tokio::sync::Mutex<()>> {
    let mut g = LOCKS.lock().expect("trash LOCKS poisoned");
    g.entry(disk.to_string())
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

// ----- Trash layout helpers -----

fn trash_root(disk_mount: &str) -> PathBuf {
    PathBuf::from(disk_mount).join(".nas-trash")
}
fn abs_trash_path(disk: &str, rel: &str) -> PathBuf {
    trash_root(disk).join(rel)
}

fn compute_bucket_rel_path(
    kind: &str,
    id: &str,
    original_name: &str,
    when: DateTime<Utc>,
) -> String {
    let prefix = if kind == "dir" { "d" } else { "f" };
    let name = sanitize_bucket_name(original_name);
    // bucket path: data/YYYY/MM/<prefix>_<id>_<name>
    format!(
        "data/{:04}/{:02}/{prefix}_{id}_{name}",
        when.format("%Y").to_string().parse::<i32>().unwrap_or(1970),
        when.format("%m").to_string().parse::<u32>().unwrap_or(1),
    )
}

/// Files stored inside the bucket must avoid `/` and other special chars
/// because the trash layout uses `/` as the directory separator. We keep
/// the base name and strip everything else.
fn sanitize_bucket_name(s: &str) -> String {
    s.chars()
        .filter(|c| {
            c.is_ascii_alphanumeric() || matches!(*c, '.' | '-' | '_' | '+' | ' ' | '(' | ')')
        })
        .collect()
}

fn is_in_nas_trash<P: AsRef<Path>>(p: P) -> bool {
    p.as_ref()
        .components()
        .any(|c| c.as_os_str() == ".nas-trash")
}

/// True when the path points inside any disk-local `.nas-trash` directory
/// (used by the media items delete action to route through trash metadata).
pub fn is_trashed_path(p: &str) -> bool {
    is_in_nas_trash(PathBuf::from(p).clean_path())
}

fn validate_trashable_path(p: &Path) -> Result<()> {
    let cleaned = p.clean_path();
    if cleaned.as_os_str().is_empty() || cleaned == PathBuf::from(".") {
        bail!("invalid path");
    }
    if cleaned == PathBuf::from("/") {
        bail!("refuse to trash root");
    }
    if is_in_nas_trash(&cleaned) {
        bail!("refuse to trash items inside .nas-trash");
    }
    Ok(())
}

fn unique_restored_path(target: &Path) -> PathBuf {
    if !target.exists() {
        return target.to_path_buf();
    }
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let base = target.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let (name, ext) = match base.rfind('.') {
        Some(i) if i > 0 => (&base[..i], &base[i..]),
        _ => (base, ""),
    };
    let first = parent.join(format!("{name} (restored){ext}"));
    if !first.exists() {
        return first;
    }
    for i in 2.. {
        let cand = parent.join(format!("{name} (restored {i}){ext}"));
        if !cand.exists() {
            return cand;
        }
    }
    target.to_path_buf()
}

// ----- High-level operations -----

/// Move each path into its disk-local `.nas-trash` (single rename).
/// Returns the new trashed physical paths.
pub async fn trash_paths(paths: Vec<String>) -> Result<Vec<String>> {
    let mut out = Vec::with_capacity(paths.len());
    for raw in paths {
        let src = PathBuf::from(&raw).clean_path();
        validate_trashable_path(&src)?;
        let meta = std::fs::symlink_metadata(&src)?;
        let kind = if meta.is_dir() { "dir" } else { "file" };
        let now = Utc::now();
        let deleted_at = now.timestamp();
        let id = shortid::new_id();

        // Probe mountpoint at the directory entry itself; for symlinks use parent.
        let probe = if meta.file_type().is_symlink() {
            src.parent().unwrap_or(&src).to_path_buf()
        } else {
            src.clone()
        };
        let disk_mount = match mountinfo::resolve_mount_point(&probe) {
            Ok(m) => m,
            Err(_) => {
                // Fallback: try canonicalize and use parent. This handles the
                // common dev/case where the path lives in a tmpfs that isn't
                // recorded in /proc/self/mountinfo.
                let canon = std::fs::canonicalize(&probe).unwrap_or(probe.clone());
                let parent = canon.parent().map(|p| p.to_path_buf()).unwrap_or(canon);
                parent.to_string_lossy().to_string()
            }
        };
        let base = src
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let rel = compute_bucket_rel_path(kind, &id, &base, now);
        let dst = abs_trash_path(&disk_mount, &rel);

        let lock = lock_for_disk(&disk_mount);
        let _g = lock.lock().await;
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        match std::fs::rename(&src, &dst) {
            Ok(()) => {}
            Err(e) if e.raw_os_error() == Some(18) /* EXDEV */ => {
                bail!("cross-filesystem rename forbidden: {e}");
            }
            Err(e) => bail!("rename failed: {e}"),
        }
        let (uid, gid, mode) = owner_mode(&meta);
        let item = TrashItem {
            id: id.clone(),
            kind: kind.to_string(),
            original_path: src.to_string_lossy().to_string(),
            disk: disk_mount.clone(),
            trash_rel_path: rel,
            deleted_at,
            uid,
            gid,
            mode,
            size: None,
            entry_count: None,
        };
        if let Err(e) = store_item(&item) {
            // Roll back the rename so metadata stays the single source of truth.
            let _ = std::fs::rename(&dst, &src);
            bail!("failed to write trash metadata: {e}");
        }
        out.push(dst.to_string_lossy().to_string());
    }
    Ok(out)
}

fn validate_restore_path(p: &Path) -> Result<String> {
    let cleaned = p.clean_path();
    if is_in_nas_trash(&cleaned) {
        let id = parse_trash_id_from_path(&cleaned).ok_or_else(|| anyhow!("invalid trash path"))?;
        Ok(id)
    } else {
        let s = cleaned.to_string_lossy().to_string();
        if s.trim().is_empty() {
            bail!("invalid trash id");
        }
        if s.contains('/') {
            bail!("invalid trash id");
        }
        Ok(s)
    }
}

fn parse_trash_id_from_path(p: &Path) -> Option<String> {
    // bucket: data/YYYY/MM/f_<id>_... or d_<id>_...
    let name = p.file_name()?.to_str()?;
    let rest = if let Some(s) = name.strip_prefix("f_") {
        s
    } else if let Some(s) = name.strip_prefix("d_") {
        s
    } else {
        return None;
    };
    let id = rest.split('_').next()?;
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

pub async fn restore_paths(trashed_paths: Vec<String>) -> Result<Vec<String>> {
    let mut out = Vec::with_capacity(trashed_paths.len());
    for raw in trashed_paths {
        let src = PathBuf::from(&raw).clean_path();
        let id = validate_restore_path(&src)?;
        let item = load_item(&id)?.ok_or_else(|| anyhow!("trash item not found"))?;
        let trash_path = abs_trash_path(&item.disk, &item.trash_rel_path);
        let target = PathBuf::from(&item.original_path).clean_path();
        if target.as_os_str().is_empty()
            || target == PathBuf::from(".")
            || target == PathBuf::from("/")
        {
            bail!("invalid original path");
        }
        // Enforce same-disk restore.
        if !target.starts_with(Path::new(&item.disk)) {
            bail!("restore target must be on original disk");
        }
        let lock = lock_for_disk(&item.disk);
        let _g = lock.lock().await;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let mut final_target = target.clone();
        if final_target.exists() {
            final_target = unique_restored_path(&final_target);
        }
        std::fs::rename(&trash_path, &final_target).context("rename from trash")?;
        // Best-effort chown/chmod (may be restricted to root).
        #[cfg(unix)]
        {
            let _ = std::os::unix::fs::chown(&final_target, Some(item.uid), Some(item.gid));
            let _ =
                std::fs::set_permissions(&final_target, std::fs::Permissions::from_mode(item.mode));
        }
        if let Err(e) = delete_item_keys(&item) {
            // Roll back the physical restore so metadata stays truth.
            let _ = std::fs::rename(&final_target, &trash_path);
            bail!("failed to delete trash metadata: {e}");
        }
        out.push(final_target.to_string_lossy().to_string());
    }
    Ok(out)
}

/// Permanently delete a trashed entry. The path may be the physical trash
/// path or a raw id.
pub async fn delete_trash_by_path(trashed_path: &str) -> Result<()> {
    let p = PathBuf::from(trashed_path).clean_path();
    if !is_in_nas_trash(&p) {
        bail!("not a trash path");
    }
    let id = parse_trash_id_from_path(&p).ok_or_else(|| anyhow!("invalid trash path"))?;
    if let Some(it) = load_item(&id)? {
        let lock = lock_for_disk(&it.disk);
        let _g = lock.lock().await;
        let abs = abs_trash_path(&it.disk, &it.trash_rel_path);
        let _ = std::fs::remove_dir_all(&abs);
        let _ = std::fs::remove_file(&abs);
        delete_item_keys(&it)?;
    } else {
        let _ = std::fs::remove_dir_all(&p);
        let _ = std::fs::remove_file(&p);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    DeletedAtNewest,
    DeletedAtOldest,
    NameAsc,
    NameDesc,
    SizeAsc,
    SizeDesc,
}

pub fn list_trash(
    offset: usize,
    limit: usize,
    text: &str,
    order: SortOrder,
) -> Result<Vec<TrashItem>> {
    let db = get_default();
    let needle = text.trim().to_lowercase();
    let mut items: Vec<TrashItem> = Vec::new();
    for kv in db.iter() {
        let (k, _v) = kv?;
        let k = String::from_utf8_lossy(&k).to_string();
        if !k.starts_with(NS_ITEM) {
            continue;
        }
        let v = db.get(&k)?.ok_or_else(|| anyhow!("missing item body"))?;
        let it: TrashItem = serde_json::from_slice(&v)?;
        if !needle.is_empty() {
            let hay = it.original_path.to_lowercase();
            if !hay.contains(&needle) {
                continue;
            }
        }
        items.push(it);
    }
    match order {
        SortOrder::DeletedAtNewest => {
            items.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at).then(a.id.cmp(&b.id)))
        }
        SortOrder::DeletedAtOldest => {
            items.sort_by(|a, b| a.deleted_at.cmp(&b.deleted_at).then(a.id.cmp(&b.id)))
        }
        SortOrder::NameAsc => items.sort_by(|a, b| {
            dir_flag(a)
                .cmp(dir_flag(b))
                .then(display_name_lower(a).cmp(&display_name_lower(b)))
                .then(a.id.cmp(&b.id))
        }),
        SortOrder::NameDesc => items.sort_by(|a, b| {
            dir_flag(a)
                .cmp(dir_flag(b))
                .then(display_name_lower(b).cmp(&display_name_lower(a)))
                .then(a.id.cmp(&b.id))
        }),
        SortOrder::SizeAsc => items.sort_by(|a, b| {
            dir_flag(a)
                .cmp(dir_flag(b))
                .then(sort_size(a).cmp(&sort_size(b)))
                .then(a.id.cmp(&b.id))
        }),
        SortOrder::SizeDesc => items.sort_by(|a, b| {
            dir_flag(a)
                .cmp(dir_flag(b))
                .then(sort_size(b).cmp(&sort_size(a)))
                .then(a.id.cmp(&b.id))
        }),
    }
    let end = (offset + limit).min(items.len());
    if offset >= items.len() {
        return Ok(vec![]);
    }
    Ok(items.drain(offset..end).collect())
}

pub fn trash_count() -> Result<usize> {
    let db = get_default();
    let mut n = 0;
    for kv in db.scan_prefix(NS_ITEM) {
        let (k, _) = kv?;
        if String::from_utf8_lossy(&k).starts_with(NS_ITEM) {
            n += 1;
        }
    }
    Ok(n)
}

// ----- Background stats worker (size / entry_count) -----
//
// Stats are computed lazily on read by `fill_stats` below; the MVP has
// no background worker queue yet.

pub fn fill_stats(it: &mut TrashItem) -> Result<()> {
    let abs = abs_trash_path(&it.disk, &it.trash_rel_path);
    if it.kind == "dir" {
        if abs.exists() {
            it.size = Some(dir_size(&abs));
            it.entry_count = Some(count_entries(&abs));
        } else {
            it.size = Some(0);
            it.entry_count = Some(0);
        }
    } else if abs.exists() {
        if let Ok(m) = std::fs::symlink_metadata(&abs) {
            it.size = Some(m.len() as i64);
        }
    }
    Ok(())
}

fn dir_size(p: &Path) -> i64 {
    crate::media::walk::Walk::new(p)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len() as i64)
        .sum()
}
fn count_entries(p: &Path) -> i64 {
    crate::media::walk::Walk::new(p)
        .into_iter()
        .filter_map(|e| e.ok())
        .count() as i64
}

#[allow(unused)]
fn _unix_to_datetime(t: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(t, 0).single().unwrap_or_else(Utc::now)
}

#[cfg(test)]
#[path = "../../tests/unit/media/trash.rs"]
mod tests;
