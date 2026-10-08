//! Public `/graphql` database-browser roots — the web console's developer
//! view over the app's SQLite file.
//!
//! The native host provides the Room database path; all browsing and SQL use
//! the shared Rust sqlite_browse core.

use super::host::Host;
use crate::content_types::Long;
use async_graphql::{Context, Enum, Object, SimpleObject};
use serde_json::Value;
use std::sync::Arc;

#[derive(SimpleObject, Clone, Debug, Default)]
pub struct DbTableInfo {
    #[graphql(name = "idKey")]
    pub id_key: String,
}

#[derive(SimpleObject, Clone, Debug, Default)]
pub struct DbTableColumn {
    pub name: String,
    #[graphql(name = "dataType")]
    pub data_type: DbColumnType,
    #[graphql(name = "notNull")]
    pub not_null: bool,
    #[graphql(name = "defaultValue")]
    pub default_value: Option<String>,
    #[graphql(name = "primaryKey")]
    pub primary_key: bool,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum DbColumnType {
    Text,
    Integer,
    Real,
    Blob,
    Numeric,
    #[default]
    Unknown,
}

impl DbColumnType {
    fn parse(value: &str) -> Self {
        match value.to_ascii_uppercase().as_str() {
            "TEXT" => Self::Text,
            "INTEGER" | "INT" => Self::Integer,
            "REAL" => Self::Real,
            "BLOB" => Self::Blob,
            "NUMERIC" => Self::Numeric,
            _ => Self::Unknown,
        }
    }
}

#[derive(Default)]
pub struct DbQuery;

#[Object]
impl DbQuery {
    async fn db_path(&self, ctx: &Context<'_>) -> async_graphql::Result<String> {
        Ok(super::public_facts::text(
            &host_call(ctx, "systemDbFacts", empty()).await?,
            "path",
        ))
    }

    async fn db_tables(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<String>> {
        let facts = host_call(ctx, "systemDbFacts", empty()).await?;
        Ok(super::public_facts::strings(&facts, "tables"))
    }

    async fn db_table_row_count(
        &self,
        ctx: &Context<'_>,
        table: String,
    ) -> async_graphql::Result<Long> {
        let count = host_call(ctx, "systemDbRowCount", table_arg(&table)).await?;
        Ok(Long(count.as_i64().unwrap_or_default()))
    }

    /// Rows come back as pre-rendered JSON strings, one per row — the
    /// column set differs per table, so there is no single row type to
    /// model.
    async fn db_table_rows(
        &self,
        ctx: &Context<'_>,
        table: String,
        offset: i32,
        limit: i32,
    ) -> async_graphql::Result<Vec<String>> {
        let facts = host_call(
            ctx,
            "systemDbRows",
            serde_json::json!({ "table": table, "offset": offset, "limit": limit }),
        )
        .await?;
        Ok(super::public_facts::strings(&facts, "rows"))
    }

    async fn db_table_columns(
        &self,
        ctx: &Context<'_>,
        table: String,
    ) -> async_graphql::Result<Vec<DbTableColumn>> {
        let facts = host_call(ctx, "systemDbColumns", table_arg(&table)).await?;
        Ok(super::public_facts::list(&facts, "columns", |column| {
            DbTableColumn {
                name: super::public_facts::text(column, "name"),
                data_type: DbColumnType::parse(&super::public_facts::text(column, "dataType")),
                not_null: super::public_facts::flag(column, "notNull"),
                default_value: column["defaultValue"]
                    .as_str()
                    .map(|value| value.to_string()),
                primary_key: super::public_facts::flag(column, "primaryKey"),
            }
        }))
    }

    async fn db_table_info(
        &self,
        ctx: &Context<'_>,
        table: String,
    ) -> async_graphql::Result<DbTableInfo> {
        let facts = host_call(ctx, "systemDbInfo", table_arg(&table)).await?;
        Ok(DbTableInfo {
            id_key: super::public_facts::text(&facts, "idKey"),
        })
    }
}

#[derive(Default)]
pub struct DbMutation;

#[Object]
impl DbMutation {
    /// `row` is a JSON object of column values. A malformed one is reported
    /// as `false` rather than an error: this is a developer console, and the
    /// operator is the one who typed it.
    async fn create_db_table_row(
        &self,
        ctx: &Context<'_>,
        table: String,
        row: String,
    ) -> async_graphql::Result<bool> {
        let row = host_call(
            ctx,
            "systemCreateDbRow",
            serde_json::json!({ "table": table, "row": row }),
        )
        .await?;
        Ok(row.as_bool().unwrap_or(false))
    }

    async fn delete_db_table_rows(
        &self,
        ctx: &Context<'_>,
        table: String,
        ids: Vec<String>,
    ) -> async_graphql::Result<bool> {
        if ids.is_empty() {
            return Ok(false);
        }
        let deleted = host_call(
            ctx,
            "systemDeleteDbRows",
            serde_json::json!({ "table": table, "ids": ids }),
        )
        .await?;
        Ok(deleted.as_bool().unwrap_or(false))
    }
}

fn empty() -> Value {
    serde_json::json!({})
}

fn table_arg(table: &str) -> Value {
    serde_json::json!({ "table": table })
}

async fn host_call(ctx: &Context<'_>, method: &str, params: Value) -> async_graphql::Result<Value> {
    let host=ctx.data_unchecked::<Arc<Host>>();
    let path=host.call("systemDbPath",empty()).await.map_err(async_graphql::Error::new)?.as_str().unwrap_or_default().to_owned();
    let method=method.to_owned();
    tokio::task::spawn_blocking(move||{let result=execute(&path,&method,&params);if matches!(method.as_str(),"systemCreateDbRow"|"systemDeleteDbRows") {Ok(result.unwrap_or(serde_json::json!(false)))}else{result}}).await.map_err(|e|async_graphql::Error::new(e.to_string()))?.map_err(|e|async_graphql::Error::new(e.to_string()))
}
fn execute(path:&str,method:&str,params:&Value)->anyhow::Result<Value> {
    use crate::sqlite_browse as browse;
    use browse::rusqlite::{Connection,OpenFlags};
    if path.is_empty(){return Ok(match method {"systemDbFacts"=>serde_json::json!({"path":"","tables":[]}),"systemDbRowCount"=>serde_json::json!(0),"systemDbRows"=>serde_json::json!({"rows":[]}),"systemDbColumns"=>serde_json::json!({"columns":[]}),"systemDbInfo"=>serde_json::json!({"idKey":"id"}),_=>serde_json::json!(false)});}
    let conn=Connection::open_with_flags(path,OpenFlags::SQLITE_OPEN_READ_WRITE)?;conn.busy_timeout(std::time::Duration::from_secs(5))?;
    if method=="systemDbFacts" {let tables:Vec<_>=browse::tables(&conn).into_iter().filter(|name|!name.starts_with("android_")&&!name.starts_with("room_")).collect();return Ok(serde_json::json!({"path":path,"tables":tables}));}
    let table=params["table"].as_str().unwrap_or_default();
    anyhow::ensure!(browse::table_exists(&conn,table),"Table not found or invalid identifier: {table}");
    Ok(match method {
        "systemDbRowCount"=>serde_json::json!(browse::row_count(&conn,table)?),
        "systemDbRows"=>serde_json::json!({"rows":browse::rows_page_text(&conn,table,params["offset"].as_i64().unwrap_or_default(),params["limit"].as_i64().unwrap_or_default())?}),
        "systemDbColumns"=>serde_json::json!({"columns":browse::table_columns(&conn,table).into_iter().map(|column|serde_json::json!({"name":column.name,"dataType":column.data_type,"notNull":column.not_null,"defaultValue":column.default_value,"primaryKey":column.primary_key})).collect::<Vec<_>>()}),
        "systemDbInfo"=>serde_json::json!({"idKey":browse::primary_key_column(&conn,table).unwrap_or_else(||"id".into())}),
        "systemCreateDbRow"=>{let mut row:serde_json::Map<String,Value>=serde_json::from_str(params["row"].as_str().unwrap_or_default())?;for value in row.values_mut(){if value.is_null(){*value=Value::String("null".into());}}browse::insert_row(&conn,table,&row)?;serde_json::json!(true)},
        "systemDeleteDbRows"=>{let ids:Vec<String>=serde_json::from_value(params["ids"].clone())?;anyhow::ensure!(!ids.is_empty(),"ids must not be empty");let key=browse::primary_key_column(&conn,table).unwrap_or_else(||"id".into());browse::delete_rows(&conn,table,&key,&ids)?;serde_json::json!(true)},
        _=>anyhow::bail!("Unsupported database operation"),
    })
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/public_db.rs"]
mod tests;
