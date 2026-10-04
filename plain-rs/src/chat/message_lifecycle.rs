use crate::{
    chat::{
        channel::chat_helper::{ChannelDeliveryResult, build_status_data_json, compute_status},
        enums::{ChannelStatus, ChatStatus},
    },
    db::{
        CHANNEL_COLS, CHAT_COLS, DChannel, DChat, DPeer, Db, PEER_COLS,
        chat_store::{SaveMode, messages},
        now_iso, row_to_channel, row_to_chat, row_to_peer,
    },
};
use anyhow::{Result, bail};
use rusqlite::{OptionalExtension, params};
use sha2::{Digest, Sha256};
#[derive(serde::Serialize)]
pub struct Received {
    pub chat: DChat,
    pub peer: DPeer,
    pub channel: Option<DChannel>,
}
pub fn create(db: &Db, to_id: &str, channel_id: &str, content: &str) -> Result<DChat> {
    if !to_id.is_empty() && !channel_id.is_empty() {
        bail!("Ambiguous chat target");
    }
    let mut chat = DChat::new("me", to_id, channel_id, content);
    if !channel_id.is_empty() || (!to_id.is_empty() && to_id != "local") {
        chat.status = ChatStatus::Pending;
    }
    messages::save(db, std::slice::from_ref(&chat), SaveMode::Insert)?;
    Ok(chat)
}
pub fn receive(
    db: &Db,
    from_id: &str,
    channel_id: &str,
    content: &str,
    signature: &str,
    timestamp: i64,
) -> Result<Option<Received>> {
    let raw = crate::base64_decode(signature);
    let now = crate::chat::pairing::now_ms();
    if raw.len() != 64
        || timestamp <= 0
        || now.abs_diff(timestamp) > super::peer_auth::TIMESTAMP_WINDOW_MS
    {
        bail!("Invalid message receipt");
    }
    let receipt = crate::base64_encode(&Sha256::digest(serde_json::to_vec(&(
        from_id,
        channel_id,
        crate::base64_encode(&raw),
        timestamp,
    ))?));
    db.with_conn(|c|->Result<Option<Received>>{
        let tx = c.unchecked_transaction()?;

        let peer=tx.query_row(&format!("SELECT {PEER_COLS} FROM peers WHERE id=?1"),[from_id],row_to_peer).optional()?.ok_or_else(||anyhow::anyhow!("invalid peer"))?;
        let channel=if channel_id.is_empty() {None}else {
            let channel=tx.query_row(&format!("SELECT {CHANNEL_COLS} FROM chat_channels WHERE id=?1"),[channel_id],row_to_channel).optional()?.ok_or_else(||anyhow::anyhow!("Unknown channel"))?;
            if channel.status!=ChannelStatus::Joined {bail!("Channel not joined");}Some(channel)
        };
        messages::validate_content(content)?;
        tx.execute("DELETE FROM chat_receipts WHERE timestamp_ms<?1",[now.saturating_sub(super::peer_auth::TIMESTAMP_WINDOW_MS as i64)])?;
        if tx.execute("INSERT INTO chat_receipts(receipt_id,peer_id,timestamp_ms) VALUES(?1,?2,?3) ON CONFLICT(receipt_id) DO NOTHING",params![receipt,from_id,timestamp])?==0 {tx.commit()?;
            return Ok(None);}
        let chat=DChat::new(from_id,if channel_id.is_empty(){"me"}else{""},channel_id,content);
        tx.execute("INSERT INTO chats(id,from_id,to_id,channel_id,content,status,status_data,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![chat.id,chat.from_id,chat.to_id,chat.channel_id,chat.content,chat.status,chat.status_data,chat.created_at,chat.updated_at])?;
        tx.commit()?;
        Ok(Some(Received{chat,peer,channel}))
    })
}
#[derive(serde::Deserialize)]
struct StatusData {
    results: Option<Vec<ChannelDeliveryResult>>,
}
pub fn delivery(
    db: &Db,
    id: &str,
    results: Option<Vec<ChannelDeliveryResult>>,
    retry: bool,
) -> Result<Option<DChat>> {
    delivery_impl(db, id, results, retry, None)
}
pub(super) fn delivery_for_content(
    db: &Db,
    id: &str,
    results: Option<Vec<ChannelDeliveryResult>>,
    retry: bool,
    content: &str,
) -> Result<Option<DChat>> {
    delivery_impl(db, id, results, retry, Some(content))
}
fn delivery_impl(
    db: &Db,
    id: &str,
    results: Option<Vec<ChannelDeliveryResult>>,
    retry: bool,
    expected_content: Option<&str>,
) -> Result<Option<DChat>> {
    db.with_conn(|c| -> Result<Option<DChat>> {
        let tx = c.unchecked_transaction()?;
        let Some(mut chat) = tx
            .query_row(
                &format!("SELECT {CHAT_COLS} FROM chats WHERE id=?1"),
                [id],
                row_to_chat,
            )
            .optional()?
        else {
            return Ok(None);
        };
        if expected_content.is_some_and(|content| content != chat.content) {
            bail!("Message content changed during delivery");
        }
        let (status, data) = if let Some(mut results) = results {
            let mut ids = std::collections::HashSet::new();
            if results
                .iter()
                .any(|r| r.peer_id.is_empty() || !ids.insert(r.peer_id.clone()))
            {
                bail!("Invalid delivery recipients");
            }
            if retry && !chat.status_data.is_empty() {
                let old: StatusData = serde_json::from_str(&chat.status_data)?;
                let mut merged: Vec<_> = old
                    .results
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|r| !ids.contains(&r.peer_id))
                    .collect();
                merged.append(&mut results);
                results = merged;
            }
            (compute_status(&results), build_status_data_json(&results))
        } else {
            (ChatStatus::Failed, String::new())
        };
        chat.status = status;
        chat.status_data = data;
        chat.updated_at = now_iso();
        tx.execute(
            "UPDATE chats SET status=?2,status_data=?3,updated_at=?4 WHERE id=?1",
            params![chat.id, chat.status, chat.status_data, chat.updated_at],
        )?;
        tx.commit()?;
        Ok(Some(chat))
    })
}
#[cfg(test)]
#[path = "../../tests/unit/chat/message_lifecycle.rs"]
mod tests;
