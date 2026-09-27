//! Row IO for `favorite_folders` — (rootPath, relativePath) pairs with
//! an optional alias, insertion order preserved.

use rusqlite::params;

use super::LibraryDb;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FavoriteFolderRow {
    pub root_path: String,
    pub relative_path: String,
    pub alias: Option<String>,
}

pub fn all_folders(db: &LibraryDb) -> Vec<FavoriteFolderRow> {
    db.with_conn(|conn| {
        let mut stmt = match conn.prepare(
            "SELECT root_path,relative_path,alias FROM favorite_folders ORDER BY rowid ASC",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        stmt.query_map(params![], |row| {
            Ok(FavoriteFolderRow {
                root_path: row.get(0)?,
                relative_path: row.get(1)?,
                alias: row.get(2)?,
            })
        })
        .ok()
        .map(|iter| iter.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    })
}

pub fn folder_by_paths(
    db: &LibraryDb,
    root_path: &str,
    relative_path: &str,
) -> Option<FavoriteFolderRow> {
    db.with_conn(|conn| {
        conn.query_row(
            "SELECT root_path,relative_path,alias FROM favorite_folders WHERE root_path=? AND relative_path=?",
            params![root_path, relative_path],
            |row| {
                Ok(FavoriteFolderRow {
                    root_path: row.get(0)?,
                    relative_path: row.get(1)?,
                    alias: row.get(2)?,
                })
            },
        )
        .ok()
    })
}

pub fn insert_folder(db: &LibraryDb, row: &FavoriteFolderRow) {
    db.with_conn(|conn| {
        let _ = conn.execute(
            "INSERT INTO favorite_folders (root_path,relative_path,alias) VALUES (?1,?2,?3)",
            params![row.root_path, row.relative_path, row.alias],
        );
    })
}

/// Remove one row, returning it when it existed.
pub fn remove_folder(
    db: &LibraryDb,
    root_path: &str,
    relative_path: &str,
) -> Option<FavoriteFolderRow> {
    let existing = folder_by_paths(db, root_path, relative_path);
    db.with_conn(|conn| {
        let _ = conn.execute(
            "DELETE FROM favorite_folders WHERE root_path=? AND relative_path=?",
            params![root_path, relative_path],
        );
    });
    existing
}

/// Set or clear (NULL) the alias. Returns whether the row exists.
pub fn set_folder_alias(
    db: &LibraryDb,
    root_path: &str,
    relative_path: &str,
    alias: Option<&str>,
) -> bool {
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE favorite_folders SET alias=?1 WHERE root_path=?2 AND relative_path=?3",
            params![alias, root_path, relative_path],
        )
        .map(|n| n > 0)
        .unwrap_or(false)
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/library/db/favorite_folder.rs"]
mod tests;
