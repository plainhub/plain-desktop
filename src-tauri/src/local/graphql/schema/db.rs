use async_graphql::{Context, Enum, Object, SimpleObject};
use std::sync::Arc;

use super::super::context::AppCtx;
use plain_rs::sqlite_browse::{self, SqliteColumnType};

#[derive(SimpleObject, Default)]
pub struct DbTableInfo {
    pub id_key: String,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Default)]
pub enum DbColumnType {
    Text,
    Integer,
    Real,
    Blob,
    Numeric,
    #[default]
    Unknown,
}

fn column_type(declared: &str) -> DbColumnType {
    match sqlite_browse::column_type_of(declared) {
        SqliteColumnType::Text => DbColumnType::Text,
        SqliteColumnType::Integer => DbColumnType::Integer,
        SqliteColumnType::Real => DbColumnType::Real,
        SqliteColumnType::Blob => DbColumnType::Blob,
        SqliteColumnType::Numeric => DbColumnType::Numeric,
        SqliteColumnType::Unknown => DbColumnType::Unknown,
    }
}

#[derive(Default)]
pub struct DbQuery;

#[Object]
impl DbQuery {
    async fn db_path(&self, ctx: &Context<'_>) -> String {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.data_dir
            .join("local_chat.db")
            .to_string_lossy()
            .into_owned()
    }

    async fn db_tables(&self, ctx: &Context<'_>) -> Vec<String> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.db.with_conn(sqlite_browse::tables)
    }

    async fn db_table_row_count(&self, ctx: &Context<'_>, table: String) -> i32 {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.db.with_conn(|conn| sqlite_browse::row_count(conn, &table).unwrap_or(0)) as i32
    }

    async fn db_table_rows(
        &self,
        ctx: &Context<'_>,
        table: String,
        offset: i32,
        limit: i32,
    ) -> Vec<String> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.db.with_conn(|conn| {
            sqlite_browse::rows_page(conn, &table, offset as i64, limit as i64).unwrap_or_default()
        })
    }

    async fn db_table_info(
        &self,
        ctx: &Context<'_>,
        table: String,
    ) -> Result<DbTableInfo, async_graphql::Error> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        Ok(DbTableInfo {
            id_key: c.db.primary_key_column(&table),
        })
    }

    async fn db_table_columns(&self, ctx: &Context<'_>, table: String) -> Vec<DbTableColumn> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.db.table_columns(&table)
            .into_iter()
            .map(|col| DbTableColumn {
                name: col.name,
                data_type: column_type(&col.data_type),
                not_null: col.not_null,
                default_value: col.default_value,
                primary_key: col.primary_key,
            })
            .collect()
    }
}

#[derive(SimpleObject, Default)]
pub struct DbTableColumn {
    pub name: String,
    pub data_type: DbColumnType,
    pub not_null: bool,
    pub default_value: Option<String>,
    pub primary_key: bool,
}

#[derive(Default)]
pub struct DbMutation;

#[Object]
impl DbMutation {
    async fn create_db_table_row(&self, ctx: &Context<'_>, table: String, row: String) -> bool {
        let Ok(obj) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&row)
        else {
            return false;
        };
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.db.with_conn(|conn| sqlite_browse::insert_row(conn, &table, &obj).is_ok())
    }

    async fn delete_db_table_rows(
        &self,
        ctx: &Context<'_>,
        table: String,
        ids: Vec<String>,
    ) -> bool {
        if ids.is_empty() {
            return false;
        }
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let id_key = c.db.primary_key_column(&table);
        c.db.with_conn(|conn| sqlite_browse::delete_rows(conn, &table, &id_key, &ids).is_ok())
    }
}
