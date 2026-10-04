use super::super::chat::{CHAT_COLS, row_to_chat};
use super::{SaveMode, validate};
use crate::db::{DChat, Db};
use anyhow::{Result, bail};
use rusqlite::{OptionalExtension, params};
pub fn get(db: &Db, id: &str) -> Result<Option<DChat>> {
    Ok(db.with_conn(|c| {
        c.query_row(
            &format!("SELECT {CHAT_COLS} FROM chats WHERE id=?1"),
            [id],
            row_to_chat,
        )
        .optional()
    })?)
}
pub fn all(db: &Db) -> Result<Vec<DChat>> {
    Ok(db.with_conn(|c| {
        let mut s = c.prepare(&format!(
            "SELECT {CHAT_COLS} FROM chats ORDER BY julianday(created_at),id"
        ))?;
        s.query_map([], row_to_chat)?
            .collect::<rusqlite::Result<Vec<_>>>()
    })?)
}
pub fn save(db: &Db, rows: &[DChat], mode: SaveMode) -> Result<()> {
    for row in rows {
        validate(&row.id, &row.created_at, &row.updated_at)?;
        validate_content(&row.content)?;
    }
    db.with_conn(|c|->Result<()>{ let tx=c.unchecked_transaction()?;
 for row in rows {
  let sql=match mode { SaveMode::Insert=>"INSERT INTO chats(id,from_id,to_id,channel_id,content,status,status_data,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)", SaveMode::Update=>"UPDATE chats SET from_id=?2,to_id=?3,channel_id=?4,content=?5,status=?6,status_data=?7,created_at=?8,updated_at=?9 WHERE id=?1",SaveMode::Upsert=>"INSERT INTO chats(id,from_id,to_id,channel_id,content,status,status_data,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(id) DO UPDATE SET from_id=excluded.from_id,to_id=excluded.to_id,channel_id=excluded.channel_id,content=excluded.content,status=excluded.status,status_data=excluded.status_data,updated_at=excluded.updated_at" };
  let changed=tx.execute(sql,params![row.id,row.from_id,row.to_id,row.channel_id,row.content,row.status,row.status_data,row.created_at,row.updated_at])?;
  if changed!=1 { bail!("messages record missing"); }
 } tx.commit()?; Ok(()) })
}
pub fn delete(db: &Db, ids: &[String]) -> Result<usize> {
    db.with_conn(|c| -> Result<usize> {
        let tx = c.unchecked_transaction()?;
        let mut n = 0;
        for id in ids {
            n += tx.execute("DELETE FROM chats WHERE id=?1", [id])?;
        }
        tx.commit()?;
        Ok(n)
    })
}
pub(crate) fn validate_content(content: &str) -> Result<()> {
    let value: serde_json::Value = serde_json::from_str(content)?;
    if !matches!(
        value.get("type").and_then(|v| v.as_str()),
        Some("TEXT" | "IMAGES" | "FILES" | "SHARE")
    ) || !value.get("value").is_some_and(|v| v.is_object())
    {
        bail!("invalid chat content envelope");
    }
    Ok(())
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Filter {
    pub peer: Option<String>,
    pub channel: Option<String>,
    pub text: String,
    pub offset: i64,
    pub limit: Option<i64>,
    pub descending: bool,
    pub latest: bool,
    pub count_only: bool,
}
pub fn list(db: &Db, filter: &Filter) -> Result<serde_json::Value> {
    if filter.offset < 0
        || filter.limit.is_some_and(|v| v < 0)
        || (filter.peer.is_some() && filter.channel.is_some())
    {
        bail!("invalid chat pagination or target");
    }
    let pattern = format!(
        "%{}%",
        filter
            .text
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    let condition = "(?1 IS NULL OR (channel_id='' AND (from_id=?1 OR to_id=?1))) AND (?2 IS NULL OR channel_id=?2) AND (?3='' OR content LIKE ?4 ESCAPE '\\')";
    let latest = if filter.latest {
        " AND julianday(c.created_at)=(SELECT max(julianday(other.created_at)) FROM chats other WHERE (c.channel_id<>'' AND other.channel_id=c.channel_id) OR (c.channel_id='' AND other.channel_id='' AND other.from_id=c.from_id AND other.to_id=c.to_id))"
    } else {
        ""
    };
    db.with_conn(|c|->Result<serde_json::Value>{
 if filter.count_only { return Ok(serde_json::json!(c.query_row(&format!("SELECT count(*) FROM chats c WHERE {condition}{latest}"),params![filter.peer,filter.channel,filter.text,pattern],|r|r.get::<_,i64>(0))?)); }
 let order=if filter.descending || filter.latest {"DESC"}else{"ASC"};
 let mut s=c.prepare(&format!("SELECT {CHAT_COLS} FROM chats c WHERE {condition}{latest} ORDER BY julianday(created_at) {order},id {order} LIMIT ?5 OFFSET ?6"))?;
 let rows=s.query_map(params![filter.peer,filter.channel,filter.text,pattern,filter.limit.unwrap_or(-1),filter.offset],row_to_chat)?.collect::<rusqlite::Result<Vec<_>>>()?;
 Ok(serde_json::to_value(rows)?)
 })
}
pub fn status(
    db: &Db,
    id: &str,
    status: crate::chat::enums::ChatStatus,
    data: Option<&str>,
) -> Result<bool> {
    if let Some(data) = data {
        if !data.is_empty() {
            serde_json::from_str::<serde_json::Value>(data)?;
        }
    }
    Ok(db.with_conn(|c| {
        c.execute(
            "UPDATE chats SET status=?2,status_data=coalesce(?3,status_data) WHERE id=?1",
            params![id, status, data],
        )
    })? == 1)
}
pub fn content(db: &Db, id: &str, content: &str) -> Result<bool> {
    validate_content(content)?;
    Ok(db.with_conn(|c| {
        c.execute(
            "UPDATE chats SET content=?2 WHERE id=?1",
            params![id, content],
        )
    })? == 1)
}
pub fn ids(db: &Db, query: &str) -> Result<Vec<String>> {
    let fields = crate::utils::search_dsl::parse(query);
    if let Some(field) = fields.iter().find(|f| f.name == "ids") {
        return Ok(field
            .value
            .split(',')
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
            .collect());
    }
    let channel = fields
        .iter()
        .find(|f| f.name == "channel")
        .map(|f| f.value.clone());
    let peer = if channel.is_none() {
        fields
            .iter()
            .find(|f| f.name == "peer")
            .map(|f| f.value.clone())
    } else {
        None
    };
    if channel.is_none() && peer.is_none() {
        return Ok(Vec::new());
    }
    let rows: Vec<DChat> = serde_json::from_value(list(
        db,
        &Filter {
            peer,
            channel,
            text: String::new(),
            offset: 0,
            limit: None,
            descending: false,
            latest: false,
            count_only: false,
        },
    )?)?;
    Ok(rows.into_iter().map(|row| row.id).collect())
}
