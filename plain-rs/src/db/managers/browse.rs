//! SQLite table browsing for developer/debug UIs (the `/developer/database`
//! page on plain-nas and plain-desktop). Everything operates on one
//! `&Connection` so any wrapper with a `with_conn` accessor can reuse it:
//!
//! - [`tables`] / [`table_exists`] — user tables from `sqlite_master`
//! - [`row_count`] / [`rows_page`] — count + one page of rows, each row a
//!   JSON string (`SELECT *`, natural rowid order; numbers stay numbers,
//!   blobs render as hex, NULL as null)
//! - [`table_columns`] / [`primary_key_column`] / [`column_type_of`] —
//!   `PRAGMA table_info` metadata
//! - [`delete_rows`] / [`insert_row`] — row mutations by primary key
//!
//! Table and column names are only ever interpolated after
//! [`is_safe_identifier`] validation (values always go through bound
//! parameters).

use rusqlite::Connection;

// Re-exported so consumers with a `with_conn` seam can name the
// connection type without depending on rusqlite themselves.
pub use rusqlite;

/// Hard cap for one rows page — the UIs ask for 50; the cap only stops a
/// runaway client from materialising an unbounded page.
pub const MAX_PAGE_LIMIT: i64 = 1000;

pub use crate::db::models::browse::{
    Result, SqliteBrowseError, SqliteColumnType, TableColumnMeta, column_type_of,
};

/// A string is safe to interpolate into SQL only as a plain identifier
/// (letters/digits/underscore, not starting with a digit).
pub fn is_safe_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {
            chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        _ => false,
    }
}

fn check_table(table: &str) -> Result<()> {
    if is_safe_identifier(table) {
        Ok(())
    } else {
        Err(SqliteBrowseError::Invalid(format!(
            "invalid table name: {table}"
        )))
    }
}

/// Whether `table` exists as a user table (SQLite-internal `sqlite_%`
/// tables are never browsable). Unsafe or missing names are simply
/// `false` (no error — callers gate SQL on this).
pub fn table_exists(conn: &Connection, table: &str) -> bool {
    if !is_safe_identifier(table) {
        return false;
    }
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 \
         AND name NOT LIKE 'sqlite_%'",
        [table],
        |_| Ok(()),
    )
    .is_ok()
}

/// Every user table name, sorted. SQLite-internal tables (`sqlite_%`) are
/// skipped.
pub fn tables(conn: &Connection) -> Vec<String> {
    let Ok(mut stmt) = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type = 'table' \
         AND name NOT LIKE 'sqlite_%' ORDER BY name",
    ) else {
        return vec![];
    };
    stmt.query_map([], |row| row.get::<_, String>(0))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
}

/// Row count of one table.
pub fn row_count(conn: &Connection, table: &str) -> Result<i64> {
    check_table(table)?;
    conn.query_row(&format!("SELECT COUNT(*) FROM `{table}`"), [], |row| {
        row.get::<_, i64>(0)
    })
    .map_err(SqliteBrowseError::Sqlite)
}

/// One page of a table's rows, each a JSON string built from
/// `SELECT *` in natural rowid order: integers/reals as JSON numbers,
/// text as JSON strings, blobs as hex strings, NULL as null.
/// `limit` is clamped to [`MAX_PAGE_LIMIT`], `offset` floored at 0.
pub fn rows_page(conn: &Connection, table: &str, offset: i64, limit: i64) -> Result<Vec<String>> {
    rows_page_values(
        conn,
        table,
        offset.max(0),
        limit.clamp(0, MAX_PAGE_LIMIT),
        false,
    )
}
pub fn rows_page_text(
    conn: &Connection,
    table: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<String>> {
    rows_page_values(conn, table, offset, limit, true)
}
fn rows_page_values(
    conn: &Connection,
    table: &str,
    offset: i64,
    limit: i64,
    text: bool,
) -> Result<Vec<String>> {
    check_table(table)?;
    let mut stmt = conn.prepare(&format!("SELECT * FROM `{table}` LIMIT ?1 OFFSET ?2"))?;
    let col_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let mut rows = stmt.query_map(rusqlite::params![limit, offset], |row| {
        let mut obj = serde_json::Map::new();
        for (i, col) in col_names.iter().enumerate() {
            let val: rusqlite::types::Value = row.get(i)?;
            let value = if text {
                match val {
                    rusqlite::types::Value::Null => serde_json::Value::Null,
                    rusqlite::types::Value::Text(value) => serde_json::Value::String(value),
                    rusqlite::types::Value::Integer(value) => {
                        serde_json::Value::String(value.to_string())
                    }
                    rusqlite::types::Value::Real(value) => {
                        serde_json::Value::String(value.to_string())
                    }
                    rusqlite::types::Value::Blob(value) => {
                        serde_json::Value::String(String::from_utf8_lossy(&value).into_owned())
                    }
                }
            } else {
                value_to_json(val)
            };
            obj.insert(col.clone(), value);
        }
        Ok(serde_json::Value::Object(obj).to_string())
    })?;
    let mut out = Vec::new();
    for row in &mut rows {
        out.push(row?);
    }
    Ok(out)
}

/// First declared primary key column of `table`, `None` when the table
/// declares none (or is missing). `pk` is 1, 2, … in key order; the
/// lowest wins, which is the first column of a composite key.
pub fn primary_key_column(conn: &Connection, table: &str) -> Option<String> {
    if !is_safe_identifier(table) {
        return None;
    }
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info(`{table}`)"))
        .ok()?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, i64>(5)?))
        })
        .ok()?;
    rows.flatten()
        .filter(|(_, pk)| *pk > 0)
        .min_by_key(|(_, pk)| *pk)
        .map(|(name, _)| name)
}

/// Column metadata of one table in declaration order; empty when the
/// table is missing or its name is unsafe.
pub fn table_columns(conn: &Connection, table: &str) -> Vec<TableColumnMeta> {
    if !is_safe_identifier(table) {
        return vec![];
    }
    let Ok(mut stmt) = conn.prepare(&format!("PRAGMA table_info(`{table}`)")) else {
        return vec![];
    };
    let Ok(rows) = stmt.query_map([], |row| {
        let default_value: Option<rusqlite::types::Value> = row.get(4)?;
        Ok(TableColumnMeta {
            name: row.get(1)?,
            data_type: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            not_null: row.get::<_, i64>(3)? != 0,
            default_value: default_value.map(value_to_string),
            primary_key: row.get::<_, i64>(5)? > 0,
        })
    }) else {
        return vec![];
    };
    rows.flatten().collect()
}

/// Delete rows of one table where `id_key` (usually the primary key
/// column) matches one of `ids`. On a composite-key table this deletes
/// every row sharing the first key column's value. Returns the number of
/// rows removed.
pub fn delete_rows(conn: &Connection, table: &str, id_key: &str, ids: &[String]) -> Result<usize> {
    check_table(table)?;
    if !is_safe_identifier(id_key) {
        return Err(SqliteBrowseError::Invalid(format!(
            "invalid id column: {id_key}"
        )));
    }
    if ids.is_empty() {
        return Err(SqliteBrowseError::Invalid(
            "ids must not be empty".to_string(),
        ));
    }
    let placeholders = (1..=ids.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!("DELETE FROM `{table}` WHERE `{id_key}` IN ({placeholders})");
    conn.execute(&sql, rusqlite::params_from_iter(ids.iter()))
        .map_err(SqliteBrowseError::Sqlite)
}

/// Insert one row built from a JSON object. Values bind as text (SQLite
/// column affinity coerces); `null` binds SQL NULL. Every key must be a
/// safe identifier. Returns the number of rows inserted.
pub fn insert_row(
    conn: &Connection,
    table: &str,
    row: &serde_json::Map<String, serde_json::Value>,
) -> Result<usize> {
    check_table(table)?;
    if row.is_empty() {
        return Err(SqliteBrowseError::Invalid(
            "row must not be empty".to_string(),
        ));
    }
    if !row.keys().all(|k| is_safe_identifier(k)) {
        return Err(SqliteBrowseError::Invalid(
            "row contains an invalid column name".to_string(),
        ));
    }
    let keys: Vec<&String> = row.keys().collect();
    let columns = keys
        .iter()
        .map(|k| format!("`{k}`"))
        .collect::<Vec<_>>()
        .join(", ");
    let placeholders = (1..=keys.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!("INSERT INTO `{table}` ({columns}) VALUES ({placeholders})");
    let args: Vec<Option<String>> = keys
        .iter()
        .map(|k| match &row[*k] {
            serde_json::Value::Null => None,
            serde_json::Value::String(s) => Some(s.clone()),
            other => Some(other.to_string()),
        })
        .collect();
    conn.execute(&sql, rusqlite::params_from_iter(args.iter()))
        .map_err(SqliteBrowseError::Sqlite)
}

fn value_to_json(val: rusqlite::types::Value) -> serde_json::Value {
    match val {
        rusqlite::types::Value::Null => serde_json::Value::Null,
        rusqlite::types::Value::Integer(n) => serde_json::Value::Number(n.into()),
        rusqlite::types::Value::Real(f) => serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        rusqlite::types::Value::Text(s) => serde_json::Value::String(s),
        rusqlite::types::Value::Blob(b) => {
            serde_json::Value::String(crate::utils::hex::bytes_to_hex(&b))
        }
    }
}

fn value_to_string(value: rusqlite::types::Value) -> String {
    match value {
        rusqlite::types::Value::Integer(n) => n.to_string(),
        rusqlite::types::Value::Real(f) => f.to_string(),
        rusqlite::types::Value::Text(s) => s,
        rusqlite::types::Value::Blob(b) => crate::utils::hex::bytes_to_hex(&b),
        rusqlite::types::Value::Null => String::new(),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/sqlite_browse.rs"]
mod tests;
