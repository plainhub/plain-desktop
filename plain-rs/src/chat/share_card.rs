use crate::db::{CHAT_COLS, DChat, Db, row_to_chat};
use anyhow::{Result, ensure};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Peer {
    pub id: String,
    pub ip: String,
    pub port: u16,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub share_id: String,
    pub url_token: String,
    pub peer_info: Peer,
    pub name: String,
    #[serde(default)]
    pub item_count: i64,
    #[serde(default)]
    pub total_size: i64,
    #[serde(default)]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}
fn parse(chat: &DChat) -> Result<(Value, Card)> {
    let value: Value = serde_json::from_str(&chat.content)?;
    ensure!(value["type"] == "SHARE", "Message is not a share card");
    let card = serde_json::from_value(value["value"].clone())?;
    Ok((value, card))
}
pub fn current(db: &Db, id: &str, expected: &Card) -> Result<String> {
    let row = crate::db::chat_store::messages::get(db, id)?
        .ok_or_else(|| anyhow::anyhow!("Share message removed"))?;
    let (_, card) = parse(&row)?;
    ensure!(card == *expected, "Share card changed");
    Ok(row.content)
}
pub fn refresh(
    db: &Db,
    id: &str,
    expected: &Card,
    expected_content: &str,
    host: &str,
    port: u16,
    name: &str,
    expires_at: Option<i64>,
) -> Result<(Card, Option<DChat>)> {
    db.with_conn(|conn| -> Result<(Card, Option<DChat>)> {
        let tx = conn.unchecked_transaction()?;
        let Some(mut row) = tx
            .query_row(
                &format!("SELECT {CHAT_COLS} FROM chats WHERE id=?1"),
                [id],
                row_to_chat,
            )
            .optional()?
        else {
            anyhow::bail!("Share message removed");
        };
        ensure!(row.content == expected_content, "Share card changed");
        let (mut content, current) = parse(&row)?;
        ensure!(current == *expected, "Share card changed");
        let mut fresh = current.clone();
        fresh.peer_info.ip = host.into();
        fresh.peer_info.port = port;
        if !name.is_empty() {
            fresh.name = name.into();
        }
        fresh.expires_at = expires_at
            .map(|ms| {
                chrono::DateTime::from_timestamp_millis(ms)
                    .ok_or_else(|| anyhow::anyhow!("Invalid share expiry"))
            })
            .transpose()?;
        if fresh == current {
            tx.commit()?;
            return Ok((current, None));
        }
        let value = content["value"]
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("Invalid share card"))?;
        value.insert("name".into(), Value::String(fresh.name.clone()));
        value.insert("expiresAt".into(), serde_json::to_value(fresh.expires_at)?);
        value.insert("peerInfo".into(), serde_json::to_value(&fresh.peer_info)?);
        let updated = serde_json::to_string(&content)?;
        ensure!(
            tx.execute(
                "UPDATE chats SET content=?2 WHERE id=?1 AND content=?3",
                params![id, updated, row.content]
            )? == 1,
            "Share card changed"
        );
        row.content = updated;
        tx.commit()?;
        Ok((fresh, Some(row)))
    })
}
#[cfg(test)]
#[path = "../../tests/unit/chat/share_card.rs"]
mod tests;
