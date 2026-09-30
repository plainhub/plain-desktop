use crate::db::Db;
use crate::sqlite_browse::{self as browse, TableColumnMeta};
use anyhow::{Result, bail};

fn browse_err(e: browse::SqliteBrowseError) -> anyhow::Error {
    anyhow::anyhow!("{e}")
}

fn checked(db: &Db, table: &str) -> Result<()> {
    if db.with_conn(|conn| browse::table_exists(conn, table)) {
        Ok(())
    } else {
        bail!("Table not found: {table}")
    }
}

pub fn tables(db: &Db) -> Vec<String> {
    db.with_conn(browse::tables)
}

pub fn table_row_count(db: &Db, table: &str) -> Result<i64> {
    checked(db, table)?;
    db.with_conn(|conn| browse::row_count(conn, table))
        .map_err(browse_err)
}

pub fn table_rows(db: &Db, table: &str, offset: i64, limit: i64) -> Result<Vec<String>> {
    checked(db, table)?;
    db.with_conn(|conn| browse::rows_page(conn, table, offset, limit))
        .map_err(browse_err)
}

pub fn table_id_key(db: &Db, table: &str) -> Result<String> {
    checked(db, table)?;
    Ok(db.with_conn(|conn| {
        browse::primary_key_column(conn, table).unwrap_or_else(|| "rowid".to_string())
    }))
}

pub fn table_columns(db: &Db, table: &str) -> Result<Vec<TableColumnMeta>> {
    checked(db, table)?;
    Ok(db.with_conn(|conn| browse::table_columns(conn, table)))
}

pub fn delete_table_rows(db: &Db, table: &str, ids: &[String]) -> Result<usize> {
    checked(db, table)?;
    db.with_conn(|conn| {
        let key = browse::primary_key_column(conn, table).unwrap_or_else(|| "rowid".to_string());
        browse::delete_rows(conn, table, &key, ids)
    })
    .map_err(browse_err)
}

#[cfg(test)]
#[path = "../../tests/unit/nas/devtools_sqlite.rs"]
mod tests;
