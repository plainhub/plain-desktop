use crate::graphql::resolvers::files::types::FileSortBy;
use crate::host::query::{self, Provider};
use anyhow::Result;
use plain_rs::db::{Db, MediaFileRow};
use plain_rs::enums::DataType;
use rusqlite::{params_from_iter, types::Value};

fn selection(db: &Db, data_type: DataType, text: &str) -> Result<(String, Vec<Value>)> {
    selection_with_trash(db, data_type, text, false)
}

fn selection_with_trash(
    db: &Db,
    data_type: DataType,
    text: &str,
    from_trash: bool,
) -> Result<(String, Vec<Value>)> {
    let provider = match data_type {
        DataType::Audio => Provider::Audio,
        DataType::Video => Provider::Video,
        DataType::Image => Provider::Image,
        DataType::Doc => Provider::Doc,
        _ => anyhow::bail!("Unsupported media catalog type"),
    };
    let fields = query::fields(db, text)?;
    let plan = query::plan(
        provider,
        &fields,
        &serde_json::Value::Null,
        None,
        text.is_empty(),
    )?;
    let mut args = vec![
        Value::Integer(i64::from(data_type.kind())),
        Value::Integer(i64::from(from_trash || plan.trash.unwrap_or(false))),
    ];
    args.extend(plan.args.into_iter().map(Value::Text));
    let mut clauses = vec!["is_trashed=?".to_string()];
    clauses.extend(plan.clauses);
    if plan.ids_column.is_some() {
        if plan.ids.is_empty() {
            clauses.push("0=1".into());
        } else {
            clauses.push(format!("_id IN ({})", vec!["?"; plan.ids.len()].join(",")));
            args.extend(plan.ids.into_iter().map(Value::Text));
        }
    }
    let sql = "WITH selected AS (SELECT *,id AS _id,path AS _data,file_name AS _display_name,size_bytes AS _size FROM media_files WHERE data_type=?)";
    Ok((
        format!(
            "{sql} SELECT {{columns}} FROM selected WHERE {}",
            clauses.join(" AND ")
        ),
        args,
    ))
}

pub(crate) fn rows(
    db: &Db,
    data_type: DataType,
    query: &str,
    sort: FileSortBy,
    limit: u32,
    offset: u32,
) -> Result<Vec<MediaFileRow>> {
    let (sql, mut args) = selection(db, data_type, query)?;
    let order = match sort {
        FileSortBy::DateAsc => "updated_at ASC,id",
        FileSortBy::DateDesc => "updated_at DESC,id",
        FileSortBy::SizeAsc => "size_bytes ASC,id",
        FileSortBy::SizeDesc => "size_bytes DESC,id",
        FileSortBy::NameAsc if data_type == DataType::Audio => "title COLLATE NOCASE ASC,id",
        FileSortBy::NameAsc => "file_name COLLATE NOCASE ASC,id",
        FileSortBy::NameDesc if data_type == DataType::Audio => "title COLLATE NOCASE DESC,id",
        FileSortBy::NameDesc => "file_name COLLATE NOCASE DESC,id",
        FileSortBy::TakenAtDesc => "taken_at DESC,updated_at DESC,id",
    };
    args.extend([
        Value::Integer(i64::from(limit)),
        Value::Integer(i64::from(offset)),
    ]);
    let sql = format!(
        "{} ORDER BY {order} LIMIT ? OFFSET ?",
        sql.replace("{columns}", plain_rs::db::media_file::COLUMNS)
    );
    Ok(db.with_conn(|conn| {
        let mut statement = conn.prepare(&sql)?;
        statement
            .query_map(
                params_from_iter(args.iter()),
                plain_rs::db::media_file::from_row,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()
    })?)
}

pub(crate) fn count(db: &Db, data_type: DataType, query: &str) -> Result<i64> {
    let (sql, args) = selection(db, data_type, query)?;
    Ok(db.with_conn(|conn| {
        conn.query_row(
            &sql.replace("{columns}", "COUNT(*)"),
            params_from_iter(args.iter()),
            |row| row.get(0),
        )
    })?)
}

pub(crate) fn ids(
    db: &Db,
    data_type: DataType,
    query: &str,
    from_trash: bool,
) -> Result<Vec<String>> {
    let (sql, args) = selection_with_trash(db, data_type, query, from_trash)?;
    let sql = format!(
        "{} ORDER BY updated_at DESC,id",
        sql.replace("{columns}", "id")
    );
    Ok(db.with_conn(|conn| {
        let mut statement = conn.prepare(&sql)?;
        statement
            .query_map(params_from_iter(args.iter()), |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()
    })?)
}

pub(crate) fn bucket_item_facts(db: &Db, data_type: DataType) -> Result<Vec<serde_json::Value>> {
    let (sql, args) = selection(db, data_type, "")?;
    let sql = format!(
        "{} ORDER BY updated_at DESC,id",
        sql.replace("{columns}", "bucket_id,path,size_bytes")
    );
    Ok(db.with_conn(|conn| {
        let mut statement = conn.prepare(&sql)?;
        statement
            .query_map(params_from_iter(args.iter()), |row| {
                let id: String = row.get(0)?;
                let path: String = row.get(1)?;
                let size: i64 = row.get(2)?;
                let name = std::path::Path::new(&path)
                    .parent()
                    .and_then(|parent| parent.file_name())
                    .and_then(|name| name.to_str())
                    .unwrap_or(&id);
                Ok(serde_json::json!({
                    "id":id,"name":name,"size":size,"path":path,"sortName":name
                }))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()
    })?)
}

#[cfg(test)]
#[path = "../../../tests/unit/services/files/media_catalog.rs"]
mod tests;
