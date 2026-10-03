//! Favorite folders — the file-browser pin list (plain-app
//! `FavoriteFoldersPreference`), stored as (rootPath, relativePath) rows
//! instead of a single JSON blob.
//!
//! `add` is idempotent: re-adding an existing (root, rel) pair returns
//! the existing entry. `remove` is also idempotent — removing a missing
//! entry returns a synthetic stub so GraphQL never errors. Path helpers
//! (`full_path_of`, `split_full_path`) live here so every consumer joins
//! and splits phone-contract `fullPath` values the same way.

use crate::db::Db;
use crate::db::favorite_folder::FavoriteFolderRow;
use crate::library::LibraryResult;

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
    let value = to_slash(p);
    let absolute = value.starts_with('/');
    let parts = value
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return if absolute { "/".into() } else { String::new() };
    }
    let mut result = parts.join("/");
    if absolute {
        result.insert(0, '/');
    }
    if cfg!(windows) && parts.len() == 1 && parts[0].ends_with(':') && value.ends_with('/') {
        result.push('/');
    }
    result
}
fn to_slash(p: &str) -> String {
    if cfg!(windows) {
        p.replace('\\', "/")
    } else {
        p.to_string()
    }
}

/// Add (rootPath, relativePath) to the favorites list. If the pair is
/// already present, the existing entry is returned unchanged. Otherwise
/// the new entry (with no alias) is appended.
pub fn add(db: &Db, root_path: &str, relative_path: &str) -> LibraryResult<FavoriteFolder> {
    let (root, rel) = normalize_args(root_path, relative_path);
    db.with_conn(|c| {
        let tx = c.unchecked_transaction()?;
        let row = if let Some(existing) = crate::db::favorite_folder::io::get(&tx, &root, &rel)? {
            existing
        } else {
            let row = FavoriteFolderRow {
                root_path: to_slash(&root),
                relative_path: to_slash(&rel),
                alias: None,
            };
            crate::db::favorite_folder::io::insert(&tx, &row)?;
            row
        };
        tx.commit()?;
        Ok(FavoriteFolder::from(row))
    })
}

/// Remove (rootPath, relativePath) from the favorites list. Returns the
/// removed entry on hit, or a synthetic stub (with the same root/rel, no
/// alias) on miss — the "never error" behavior of the Go side.
pub fn remove(db: &Db, root_path: &str, relative_path: &str) -> LibraryResult<FavoriteFolder> {
    let (root, rel) = normalize_args(root_path, relative_path);
    let removed = crate::db::favorite_folder::remove_folder(db, &root, &rel)?;
    Ok(match removed {
        Some(row) => FavoriteFolder::from(row),
        None => FavoriteFolder {
            root_path: to_slash(&root),
            relative_path: to_slash(&rel),
            alias: None,
        },
    })
}

/// Set the alias for (rootPath, relativePath). An empty/whitespace-only
/// alias clears the field. Returns whether a matching entry exists (the
/// Go side returns true unconditionally; callers keep that contract).
pub fn set_alias(
    db: &Db,
    root_path: &str,
    relative_path: &str,
    alias: &str,
) -> LibraryResult<bool> {
    let (root, rel) = normalize_args(root_path, relative_path);
    let alias = alias.trim();
    let alias = if alias.is_empty() { None } else { Some(alias) };
    Ok(crate::db::favorite_folder::set_folder_alias(
        db, &root, &rel, alias,
    )?)
}

/// List all favorites with paths normalized to forward slashes and
/// aliases trimmed (empty trimmed aliases collapse to None).
pub fn list(db: &Db) -> LibraryResult<Vec<FavoriteFolder>> {
    Ok(crate::db::favorite_folder::all_folders(db)?
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
        .collect())
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
/// (root, relative) pair the store keys on. Paths outside the root fail.
pub fn split_full_path(root: &str, full: &str) -> LibraryResult<(String, String)> {
    let root = clean_path(root);
    let root = if root.is_empty() {
        "/".to_string()
    } else {
        root
    };
    let full = clean_path(full);
    if std::path::Path::new(&root)
        .components()
        .chain(std::path::Path::new(&full).components())
        .any(|part| part == std::path::Component::ParentDir)
    {
        return Err(crate::library::LibraryError::Other(
            "favorite path contains parent traversal".into(),
        ));
    }
    if full == root {
        return Ok((root, String::new()));
    }
    let prefix = format!("{}/", root.trim_end_matches('/'));
    let relative = full
        .strip_prefix(&prefix)
        .ok_or_else(|| crate::library::LibraryError::Other("favorite path is outside root".into()))?
        .to_string();
    Ok((root, relative))
}

pub fn add_full_path(db: &Db, root: &str, full: &str) -> LibraryResult<FavoriteFolder> {
    let (root, rel) = split_full_path(root, full)?;
    let candidate = FavoriteFolder {
        root_path: root,
        relative_path: rel,
        alias: None,
    };
    let target = full_path_of(&candidate);
    db.with_conn(|c| {
        let tx = c.unchecked_transaction()?;
        let mut value = candidate.clone();
        for row in crate::db::favorite_folder::io::all(&tx)? {
            let existing = FavoriteFolder::from(row);
            if full_path_of(&existing) == target {
                value.alias = existing.alias;
                crate::db::favorite_folder::io::remove(
                    &tx,
                    &existing.root_path,
                    &existing.relative_path,
                )?;
            }
        }
        crate::db::favorite_folder::io::insert(
            &tx,
            &FavoriteFolderRow {
                root_path: value.root_path.clone(),
                relative_path: value.relative_path.clone(),
                alias: value.alias.clone(),
            },
        )?;
        tx.commit()?;
        Ok(value)
    })
}

/// Find the stored favorite whose joined full path equals `full`
/// (trailing-slash-insensitive).
pub fn find_by_full_path(db: &Db, full: &str) -> LibraryResult<Option<FavoriteFolder>> {
    let target = full.trim_end_matches('/');
    Ok(list(db)?.into_iter().find(|f| full_path_of(f) == target))
}

#[cfg(test)]
#[path = "../../tests/unit/library/favorite_folders.rs"]
mod tests;
