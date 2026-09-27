//! Favorite folders — the file-browser pin list (plain-app
//! `FavoriteFoldersPreference`), stored as (rootPath, relativePath) rows
//! instead of a single JSON blob.
//!
//! `add` is idempotent: re-adding an existing (root, rel) pair returns
//! the existing entry. `remove` is also idempotent — removing a missing
//! entry returns a synthetic stub so GraphQL never errors. Path helpers
//! (`full_path_of`, `split_full_path`) live here so every consumer joins
//! and splits phone-contract `fullPath` values the same way.

use crate::library::db::LibraryDb;
use crate::library::db::favorite_folder::FavoriteFolderRow;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FavoriteFolder {
    pub root_path: String,
    pub relative_path: String,
    pub alias: Option<String>,
}

impl From<FavoriteFolderRow> for FavoriteFolder {
    fn from(r: FavoriteFolderRow) -> Self {
        Self {
            root_path: r.root_path,
            relative_path: r.relative_path,
            alias: r.alias,
        }
    }
}

/// Normalize the (rootPath, relativePath) pair the same way
/// `filepath.Clean` does on the Go side. `relativePath == "."` collapses
/// to "" (the "this volume" / "root of volume" case).
fn normalize_args(root_path: &str, relative_path: &str) -> (String, String) {
    let root = clean_path(root_path);
    let mut rel = clean_path(relative_path);
    if rel == "." {
        rel.clear();
    }
    (root, rel)
}

/// Equivalent of `filepath.Clean` for our purposes:
/// - strip trailing slashes (except keep a single "/" for the root case)
/// - collapse runs of "/" into one
/// - preserve the leading "/" for absolute paths
fn clean_path(p: &str) -> String {
    let p = p.trim();
    if p.is_empty() {
        return String::new();
    }
    let is_absolute = p.starts_with('/');
    let body = p.trim_start_matches('/');
    let cleaned: Vec<String> = std::path::Path::new(body)
        .components()
        .filter_map(|c| c.as_os_str().to_str().map(|s| s.to_string()))
        .collect();
    if cleaned.is_empty() {
        return if is_absolute {
            "/".to_string()
        } else {
            String::new()
        };
    }
    let mut s = cleaned.join("/");
    if is_absolute {
        s.insert(0, '/');
    }
    s
}

/// Convert a cleaned path to a forward-slash string (Go's `filepath.ToSlash`).
fn to_slash(p: &str) -> String {
    p.replace('\\', "/")
}

/// Add (rootPath, relativePath) to the favorites list. If the pair is
/// already present, the existing entry is returned unchanged. Otherwise
/// the new entry (with no alias) is appended.
pub fn add(db: &LibraryDb, root_path: &str, relative_path: &str) -> FavoriteFolder {
    let (root, rel) = normalize_args(root_path, relative_path);
    if let Some(existing) = crate::library::db::favorite_folder::folder_by_paths(db, &root, &rel) {
        return FavoriteFolder::from(existing);
    }
    let new_item = FavoriteFolderRow {
        root_path: to_slash(&root),
        relative_path: to_slash(&rel),
        alias: None,
    };
    crate::library::db::favorite_folder::insert_folder(db, &new_item);
    FavoriteFolder::from(new_item)
}

/// Remove (rootPath, relativePath) from the favorites list. Returns the
/// removed entry on hit, or a synthetic stub (with the same root/rel, no
/// alias) on miss — the "never error" behavior of the Go side.
pub fn remove(db: &LibraryDb, root_path: &str, relative_path: &str) -> FavoriteFolder {
    let (root, rel) = normalize_args(root_path, relative_path);
    let removed = crate::library::db::favorite_folder::remove_folder(db, &root, &rel);
    match removed {
        Some(row) => FavoriteFolder::from(row),
        None => FavoriteFolder {
            root_path: to_slash(&root),
            relative_path: to_slash(&rel),
            alias: None,
        },
    }
}

/// Set the alias for (rootPath, relativePath). An empty/whitespace-only
/// alias clears the field. Returns whether a matching entry exists (the
/// Go side returns true unconditionally; callers keep that contract).
pub fn set_alias(db: &LibraryDb, root_path: &str, relative_path: &str, alias: &str) -> bool {
    let (root, rel) = normalize_args(root_path, relative_path);
    let alias = alias.trim();
    let alias = if alias.is_empty() { None } else { Some(alias) };
    crate::library::db::favorite_folder::set_folder_alias(db, &root, &rel, alias)
}

/// List all favorites with paths normalized to forward slashes and
/// aliases trimmed (empty trimmed aliases collapse to None).
pub fn list(db: &LibraryDb) -> Vec<FavoriteFolder> {
    crate::library::db::favorite_folder::all_folders(db)
        .into_iter()
        .map(|mut f| {
            f.root_path = to_slash(&f.root_path);
            f.relative_path = to_slash(&f.relative_path);
            f.alias = f.alias.take().and_then(|a| {
                let t = a.trim().to_string();
                if t.is_empty() { None } else { Some(t) }
            });
            FavoriteFolder::from(f)
        })
        .collect()
}

/// Joined `rootPath/relativePath` — the phone-contract `fullPath`
/// ("root itself" when relative is empty).
pub fn full_path_of(f: &FavoriteFolder) -> String {
    if f.relative_path.is_empty() {
        f.root_path.clone()
    } else {
        format!(
            "{}/{}",
            f.root_path.trim_end_matches('/'),
            f.relative_path.trim_start_matches('/')
        )
    }
}

/// Split a phone-contract `fullPath` against its `rootPath` into the
/// (root, relative) pair the store keys on. A fullPath outside the root
/// degrades to (root, "").
pub fn split_full_path(root: &str, full: &str) -> (String, String) {
    let root = root.trim_end_matches('/');
    let rel = full
        .trim_end_matches('/')
        .strip_prefix(root)
        .map(|r| r.trim_start_matches('/'))
        .unwrap_or("");
    (root.to_string(), rel.to_string())
}

/// Find the stored favorite whose joined full path equals `full`
/// (trailing-slash-insensitive).
pub fn find_by_full_path(db: &LibraryDb, full: &str) -> Option<FavoriteFolder> {
    let target = full.trim_end_matches('/');
    list(db).into_iter().find(|f| full_path_of(f) == target)
}

#[cfg(test)]
#[path = "../../tests/unit/library/favorite_folders.rs"]
mod tests;
