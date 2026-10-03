use crate::db::Db;
pub use crate::db::models::favorite_folder::FavoriteFolderRow;
use rusqlite::{Connection, OptionalExtension, params};
pub(crate) mod io {
    use super::*;
    fn row(value: &rusqlite::Row<'_>) -> rusqlite::Result<FavoriteFolderRow> {
        Ok(FavoriteFolderRow {
            root_path: value.get(0)?,
            relative_path: value.get(1)?,
            alias: value.get(2)?,
        })
    }
    pub fn all(c: &Connection) -> rusqlite::Result<Vec<FavoriteFolderRow>> {
        c.prepare("SELECT root_path,relative_path,alias FROM favorite_folders ORDER BY rowid")?
            .query_map([], row)?
            .collect()
    }
    pub fn get(
        c: &Connection,
        root: &str,
        rel: &str,
    ) -> rusqlite::Result<Option<FavoriteFolderRow>> {
        c.query_row("SELECT root_path,relative_path,alias FROM favorite_folders WHERE root_path=?1 AND relative_path=?2",params![root,rel],row).optional()
    }
    pub fn insert(c: &Connection, value: &FavoriteFolderRow) -> rusqlite::Result<()> {
        c.execute("INSERT INTO favorite_folders(root_path,relative_path,alias) VALUES(?1,?2,?3) ON CONFLICT(root_path,relative_path) DO NOTHING",params![value.root_path,value.relative_path,value.alias])?;
        Ok(())
    }
    pub fn remove(
        c: &Connection,
        root: &str,
        rel: &str,
    ) -> rusqlite::Result<Option<FavoriteFolderRow>> {
        let value = get(c, root, rel)?;
        c.execute(
            "DELETE FROM favorite_folders WHERE root_path=?1 AND relative_path=?2",
            params![root, rel],
        )?;
        Ok(value)
    }
}
pub fn all_folders(db: &Db) -> rusqlite::Result<Vec<FavoriteFolderRow>> {
    db.with_conn(io::all)
}
pub fn folder_by_paths(
    db: &Db,
    root: &str,
    rel: &str,
) -> rusqlite::Result<Option<FavoriteFolderRow>> {
    db.with_conn(|c| io::get(c, root, rel))
}
pub fn insert_folder(db: &Db, value: &FavoriteFolderRow) -> rusqlite::Result<()> {
    db.with_conn(|c| io::insert(c, value))
}
pub fn remove_folder(
    db: &Db,
    root: &str,
    rel: &str,
) -> rusqlite::Result<Option<FavoriteFolderRow>> {
    db.with_conn(|c| {
        let tx = c.unchecked_transaction()?;
        let value = io::remove(&tx, root, rel)?;
        tx.commit()?;
        Ok(value)
    })
}
pub fn set_folder_alias(
    db: &Db,
    root: &str,
    rel: &str,
    alias: Option<&str>,
) -> rusqlite::Result<bool> {
    db.with_conn(|c| {
        c.execute(
            "UPDATE favorite_folders SET alias=?1 WHERE root_path=?2 AND relative_path=?3",
            params![alias, root, rel],
        )
        .map(|n| n > 0)
    })
}
#[cfg(test)]
#[path = "../../../tests/unit/library/db/favorite_folder.rs"]
mod tests;
