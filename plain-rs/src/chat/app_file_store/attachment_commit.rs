use crate::db::{CHAT_COLS, DAppFile, DChat, Db, now_iso, row_to_chat};
use anyhow::{Result, bail};
use rusqlite::{OptionalExtension, params};
use serde_json::Value;
pub(super) enum Selection<'a> {
    Attachment {
        message_id: &'a str,
        id: &'a str,
        original_uri: &'a str,
    },
    LinkPreview {
        message_id: &'a str,
        text: &'a str,
        preview: &'a Value,
    },
}
pub(super) fn commit(
    db: &Db,
    file: &DAppFile,
    exists: bool,
    suffix: &str,
    attachment: Option<Selection<'_>>,
) -> Result<Option<DChat>> {
    db.with_conn(|c| {
        let tx=c.unchecked_transaction()?;
        let mut chat=None;let mut references=1i64;
        if let Some(selection)=attachment {
            let mut row=tx.query_row(&format!("SELECT {CHAT_COLS} FROM chats WHERE id=?1"),[match &selection {Selection::Attachment{message_id,..}|Selection::LinkPreview{message_id,..}=>*message_id}],row_to_chat).optional()?.ok_or_else(||anyhow::anyhow!("Message unavailable"))?;
            let mut content:Value=serde_json::from_str(&row.content)?;
            match selection {
                Selection::Attachment{id,original_uri,..}=>{
                    if !matches!(content["type"].as_str(),Some("IMAGES"|"FILES")) {bail!("Message has no attachments");}
                    let items=content["value"]["items"].as_array_mut().ok_or_else(||anyhow::anyhow!("Invalid attachment items"))?;
                    references=0;
                    for item in items.iter_mut().filter(|item|item["id"].as_str()==Some(id) && item["uri"].as_str()==Some(original_uri)) {
                        if item["size"].as_i64()!=Some(file.size) {bail!("Incomplete attachment download");}
                        item["uri"]=Value::String(format!("fid:{suffix}"));references+=1;
                    }
                    if references==0 {bail!("Attachment changed or unavailable");}
                },
                Selection::LinkPreview{text,preview,..}=>{
                    if content["type"]!="TEXT" || content["value"]["text"].as_str()!=Some(text) {bail!("Preview message changed");}
                    let mut preview=preview.clone();
                    let url=preview["url"].as_str().ok_or_else(||anyhow::anyhow!("Preview URL unavailable"))?;
                    let mut previews=content["value"]["linkPreviews"].as_array().cloned().unwrap_or_default();
                    if previews.iter().any(|value|value["url"].as_str()==Some(url)){bail!("Preview already committed");}
                    preview["imageLocalPath"]=Value::String(format!("fid:{suffix}"));
                    previews.push(preview);content["value"]["linkPreviews"]=Value::Array(previews);
                },
            }
            row.content=content.to_string();row.updated_at=now_iso();
            if tx.execute("UPDATE chats SET content=?2,updated_at=?3 WHERE id=?1",params![row.id,row.content,row.updated_at])?!=1 {bail!("Message update failed");}
            chat=Some(row);
        }
        if exists {
            if tx.execute("UPDATE app_files SET ref_count=ref_count+?2,updated_at=?3 WHERE id=?1",params![file.id,references,file.updated_at])?!=1 {bail!("App file unavailable");}
        } else {
            tx.execute("INSERT INTO app_files(id,size,mime_type,real_path,ref_count,weak_hash,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![file.id,file.size,file.mime_type,file.real_path,references,file.weak_hash,file.created_at,file.updated_at])?;
        }
        tx.commit()?;Ok(chat)
    })
}
