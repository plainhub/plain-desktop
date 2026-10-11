use crate::assembly::server::ServerState;
use anyhow::{Result, ensure};
use plain_rs::chat::events;
use plain_rs::chat::messages::message_commands;
use plain_rs::chat::messages::share_send;
use plain_rs::db::DChat;
use plain_rs::events::WsEvent;
use serde_json::json;
pub(crate) fn committed(state: &ServerState, chat: &DChat, event: &'static str) {
    state.previews.request(&chat.id);
    let _ = state.events.send(WsEvent::broadcast(
        event,
        json!([plain_rs::chat::service::chat_to_json(chat)]).to_string(),
    ));
}
pub(crate) async fn deliver(state: &ServerState, row: DChat) -> Result<DChat> {
    if row.channel_id.is_empty() && (row.to_id.is_empty() || row.to_id == "local") {
        return Ok(row);
    }
    crate::services::chat::chat_delivery::deliver(state, &row.id, None)
        .await?
        .chat
        .ok_or_else(|| anyhow::anyhow!("Message removed"))
}
pub(crate) async fn send(state: &ServerState, target: &str, content: &str) -> Result<DChat> {
    let (to, channel) = message_commands::target(target)?;
    let row =
        plain_rs::chat::messages::message_lifecycle::create(&state.db, &to, &channel, content)?;
    committed(state, &row, events::WS_MESSAGE_CREATED);
    deliver(state, row).await
}
pub(crate) async fn send_many(
    state: &ServerState,
    targets: Vec<String>,
    content: String,
) -> Result<Vec<DChat>> {
    ensure!(targets.len() <= 128, "Too many share targets");
    let targets = targets
        .iter()
        .map(|target| message_commands::target(target))
        .collect::<Result<Vec<_>>>()?;
    let rows =
        plain_rs::chat::messages::message_lifecycle::create_many(&state.db, &targets, &content)?;
    for row in &rows {
        committed(state, row, events::WS_MESSAGE_CREATED);
    }
    futures_util::future::join_all(rows.into_iter().map(|row| deliver(state, row)))
        .await
        .into_iter()
        .collect()
}

pub(crate) async fn send_text(
    state: &ServerState,
    targets: Vec<String>,
    text: &str,
) -> Result<Vec<DChat>> {
    if targets.is_empty() {
        return Ok(vec![]);
    }
    for target in &targets {
        message_commands::target(target)?;
    }
    let content = share_send::text(&state.db, &state.directory, text)?;
    let _imports = Imported {
        db: state.db.clone(),
        directory: state.directory.clone(),
        ids: plain_rs::chat::attachments::app_file_store::content_reference_ids(&content),
    };
    send_many(state, targets, content.to_string()).await
}
pub(crate) fn delete_query(state: &ServerState, query: &str) -> Result<usize> {
    let ids = if query.trim().is_empty() {
        vec![]
    } else {
        plain_rs::db::chat_store::messages::ids(&state.db, query)?
    };
    delete(state, ids)
}

pub(crate) fn delete(state: &ServerState, ids: Vec<String>) -> Result<usize> {
    let count = plain_rs::chat::attachments::app_file_store::chat_deletion::delete(
        &state.db,
        &state.directory,
        plain_rs::chat::attachments::app_file_store::chat_deletion::Selection::Ids(&ids),
    )?;
    let _ = state.events.send(WsEvent::broadcast(
        events::WS_MESSAGE_DELETED,
        json!(format!("ids={}", ids.join(","))).to_string(),
    ));
    Ok(count)
}

struct Imported {
    db: std::sync::Arc<plain_rs::db::Db>,
    directory: std::path::PathBuf,
    ids: std::collections::BTreeMap<String, i64>,
}
impl Drop for Imported {
    fn drop(&mut self) {
        if let Err(error) = plain_rs::chat::attachments::app_file_store::release_unbound(
            &self.db,
            &self.directory,
            std::mem::take(&mut self.ids),
        ) {
            log::warn!("Unable to release unused chat imports: {error}");
        }
    }
}
pub(crate) async fn retry(state: &ServerState, id: String) -> Result<DChat> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let sender = std::sync::Arc::new(std::sync::Mutex::new(Some(sender)));
    let state = state.clone();
    tokio::spawn(async move {
        let mut stop = state.stop.clone();
        let result = tokio::select! {
            _=stop.changed()=>Err(anyhow::anyhow!("Chat service stopped")),
            result=crate::services::chat::chat_delivery::deliver_observed(&state,&id,None,|chat| {
                if let Some(sender)=sender.lock().unwrap().take() {let _=sender.send(Ok(chat.clone()));}
            })=>result.and_then(|receipt|receipt.chat.ok_or_else(||anyhow::anyhow!("Message unavailable"))),
        };
        if let Some(sender) = sender.lock().unwrap().take() {
            let _ = sender.send(result);
        }
    });
    receiver.await?
}
