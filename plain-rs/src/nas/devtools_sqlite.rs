//! SQLite browsing behind the web UI's `/developer/database` page. The
//! user-facing row data lives in the two plain-rs SQLite stores under the
//! data dir — `chat.db` (chats / channels / peers / bookmarks / …) and
//! `library.db` (audio queue / playlists / history / tags / favorites) —
//! so the page browses those; the internal fjall namespaces
//! (event / media / session) are server-internal and not listed.
//!
//! Tables are listed by bare name (`chats`, `tags`, …) — no `chat.` /
//! `library.` store prefix. The two stores share no table names, so the
//! owning store is found by probing both. All actual browsing (identifier
//! guards, `PRAGMA table_info` metadata, `SELECT *` rows as JSON, delete
//! by primary key) lives in `plain_rs::sqlite_browse` — shared with
//! plain-desktop's debug DB page; this module only routes a UI table name
//! to the right store.

use crate::chat::db::ChatDb;
use crate::library::db::LibraryDb;
use crate::sqlite_browse::{self as browse, TableColumnMeta, rusqlite::Connection};
use anyhow::{Result, bail};

/// Which of the two SQLite stores a UI table belongs to.
enum DbRef<'a> {
    Chat(&'a ChatDb),
    Library(&'a LibraryDb),
}

impl DbRef<'_> {
    fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> T) -> T {
        match self {
            DbRef::Chat(db) => db.with_conn(f),
            DbRef::Library(db) => db.with_conn(f),
        }
    }
}

fn browse_err(e: browse::SqliteBrowseError) -> anyhow::Error {
    anyhow::anyhow!("{e}")
}

/// Resolve a bare table name to its owning store, checking that the table
/// exists. A name present in both stores is rejected as ambiguous (the
/// stores share no schema today; this keeps the contract honest if that
/// ever changes).
fn db_of<'a>(chat: &'a ChatDb, library: &'a LibraryDb, table: &str) -> Result<(DbRef<'a>, String)> {
    let in_chat = chat.with_conn(|conn| browse::table_exists(conn, table));
    let in_library = library.with_conn(|conn| browse::table_exists(conn, table));
    let db = match (in_chat, in_library) {
        (true, true) => bail!("ambiguous table name (exists in both stores): {table}"),
        (true, false) => DbRef::Chat(chat),
        (false, true) => DbRef::Library(library),
        (false, false) => bail!("Table not found: {table}"),
    };
    Ok((db, table.to_string()))
}

/// Table names in UI order: every user table of both stores, by bare name,
/// sorted and deduplicated.
pub fn tables(chat: &ChatDb, library: &LibraryDb) -> Vec<String> {
    let mut all = chat.with_conn(browse::tables);
    all.extend(library.with_conn(browse::tables));
    all.sort();
    all.dedup();
    all
}

/// Row count of one table.
pub fn table_row_count(chat: &ChatDb, library: &LibraryDb, table: &str) -> Result<i64> {
    let (db, name) = db_of(chat, library, table)?;
    db.with_conn(|conn| browse::row_count(conn, &name))
        .map_err(browse_err)
}

/// One page of a table's rows, as JSON strings (see
/// `plain_rs::sqlite_browse::rows_page` for the value mapping).
pub fn table_rows(
    chat: &ChatDb,
    library: &LibraryDb,
    table: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<String>> {
    let (db, name) = db_of(chat, library, table)?;
    db.with_conn(|conn| browse::rows_page(conn, &name, offset, limit))
        .map_err(browse_err)
}

/// The declared primary key column of one table (first column of a
/// composite key); `rowid` when the table declares none.
pub fn table_id_key(chat: &ChatDb, library: &LibraryDb, table: &str) -> Result<String> {
    let (db, name) = db_of(chat, library, table)?;
    Ok(db.with_conn(|conn| {
        browse::primary_key_column(conn, &name).unwrap_or_else(|| "rowid".to_string())
    }))
}

/// Column metadata of one table, declaration order.
pub fn table_columns(
    chat: &ChatDb,
    library: &LibraryDb,
    table: &str,
) -> Result<Vec<TableColumnMeta>> {
    let (db, name) = db_of(chat, library, table)?;
    Ok(db.with_conn(|conn| browse::table_columns(conn, &name)))
}

/// Delete rows of one table by their primary key values (every row
/// sharing the first key column's value on a composite-key table).
/// Returns the number of rows removed.
pub fn delete_table_rows(
    chat: &ChatDb,
    library: &LibraryDb,
    table: &str,
    ids: &[String],
) -> Result<usize> {
    let (db, name) = db_of(chat, library, table)?;
    db.with_conn(|conn| {
        let id_key = browse::primary_key_column(conn, &name).unwrap_or_else(|| "rowid".to_string());
        browse::delete_rows(conn, &name, &id_key, ids)
    })
    .map_err(browse_err)
}

#[cfg(test)]
#[path = "../../tests/unit/nas/devtools_sqlite.rs"]
mod tests;
