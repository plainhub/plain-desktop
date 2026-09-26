use async_graphql::{Context, Enum, Object, SimpleObject};
use std::sync::Arc;

use super::super::context::AppCtx;
use super::util::is_safe_identifier;
use plain_rs::hex::bytes_to_hex;

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
    match declared.to_ascii_uppercase().as_str() {
        "TEXT" => DbColumnType::Text,
        "INTEGER" => DbColumnType::Integer,
        "REAL" => DbColumnType::Real,
        "BLOB" => DbColumnType::Blob,
        "NUMERIC" => DbColumnType::Numeric,
        _ => DbColumnType::Unknown,
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
        c.db.with_conn(|conn| {
            let mut stmt = match conn.prepare(
                "SELECT name FROM sqlite_master WHERE type='table' \
                 AND name NOT LIKE 'sqlite_%' ORDER BY name",
            ) {
                Ok(s) => s,
                Err(_) => return vec![],
            };
            stmt.query_map([], |row| row.get(0))
                .map(|rows| rows.flatten().collect())
                .unwrap_or_default()
        })
    }

    async fn db_table_row_count(&self, ctx: &Context<'_>, table: String) -> i32 {
        if !is_safe_identifier(&table) {
            return 0;
        }
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.db.with_conn(|conn| {
            conn.query_row(&format!("SELECT COUNT(*) FROM `{table}`"), [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap_or(0) as i32
        })
    }

    async fn db_table_rows(
        &self,
        ctx: &Context<'_>,
        table: String,
        offset: i32,
        limit: i32,
    ) -> Vec<String> {
        if !is_safe_identifier(&table) {
            return vec![];
        }
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.db.with_conn(|conn| {
            let sql = format!("SELECT * FROM `{table}` LIMIT ?1 OFFSET ?2");
            let mut stmt = match conn.prepare(&sql) {
                Ok(s) => s,
                Err(_) => return vec![],
            };
            let col_names: Vec<String> =
                stmt.column_names().iter().map(|s| s.to_string()).collect();
            stmt.query_map(rusqlite::params![limit, offset], |row| {
                let mut obj = serde_json::Map::new();
                for (i, name) in col_names.iter().enumerate() {
                    let val: rusqlite::types::Value = row.get(i)?;
                    let json_val = match val {
                        rusqlite::types::Value::Null => serde_json::Value::Null,
                        rusqlite::types::Value::Integer(n) => serde_json::Value::Number(n.into()),
                        rusqlite::types::Value::Real(f) => serde_json::Number::from_f64(f)
                            .map(serde_json::Value::Number)
                            .unwrap_or(serde_json::Value::Null),
                        rusqlite::types::Value::Text(s) => serde_json::Value::String(s),
                        rusqlite::types::Value::Blob(b) => {
                            serde_json::Value::String(bytes_to_hex(&b))
                        }
                    };
                    obj.insert(name.clone(), json_val);
                }
                Ok(serde_json::Value::Object(obj).to_string())
            })
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
        })
    }

    async fn db_table_info(
        &self,
        ctx: &Context<'_>,
        table: String,
    ) -> Result<DbTableInfo, async_graphql::Error> {
        if !is_safe_identifier(&table) {
            return Err(async_graphql::Error::new("invalid table name"));
        }
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        Ok(DbTableInfo {
            id_key: c.db.primary_key_column(&table),
        })
    }

    async fn db_table_columns(&self, ctx: &Context<'_>, table: String) -> Vec<DbTableColumn> {
        if !is_safe_identifier(&table) {
            return vec![];
        }
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

#[derive(Default)]
pub struct DbMutation;

#[Object]
impl DbMutation {
    async fn create_db_table_row(&self, ctx: &Context<'_>, table: String, row: String) -> bool {
        if !is_safe_identifier(&table) {
            return false;
        }
        let obj: serde_json::Map<String, serde_json::Value> = match serde_json::from_str(&row) {
            Ok(m) => m,
            Err(_) => return false,
        };
        if obj.is_empty() || !obj.keys().all(|k| is_safe_identifier(k)) {
            return false;
        }
        let keys: Vec<&String> = obj.keys().collect();
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
            .map(|k| match &obj[*k] {
                serde_json::Value::Null => None,
                serde_json::Value::String(s) => Some(s.clone()),
                other => Some(other.to_string()),
            })
            .collect();
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.db.with_conn(|conn| {
            conn.execute(&sql, rusqlite::params_from_iter(args.iter()))
                .is_ok()
        })
    }

    async fn delete_db_table_rows(
        &self,
        ctx: &Context<'_>,
        table: String,
        ids: Vec<String>,
    ) -> bool {
        if !is_safe_identifier(&table) || ids.is_empty() {
            return false;
        }
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let id_key = c.db.primary_key_column(&table);
        if !is_safe_identifier(&id_key) {
            return false;
        }
        let placeholders = (1..=ids.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("DELETE FROM `{table}` WHERE `{id_key}` IN ({placeholders})");
        c.db.with_conn(|conn| {
            conn.execute(&sql, rusqlite::params_from_iter(ids.iter()))
                .is_ok()
        })
    }
}
