use crate::assembly::server::ServerState;
use anyhow::{Result, ensure};
use plain_rs::chat::events;
use plain_rs::chat::messages::message_commands;
use plain_rs::chat::messages::share_send;
use plain_rs::db::DChat;
use plain_rs::events::WsEvent;
use serde_json::{Value, json};
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
pub(crate) async fn share(
    state: &ServerState,
    targets: Vec<String>,
    attachments: Vec<share_send::Attachment>,
    text: Option<String>,
    caption: Option<String>,
    images: Option<bool>,
    normalize: bool,
) -> Result<Value> {
    ensure!(targets.len() <= 128, "Too many share targets");
    if targets.is_empty() {
        return Ok(json!({"ok":false,"chats":[]}));
    }
    for target in &targets {
        message_commands::target(target)?;
    }
    if attachments.is_empty() {
        let Some(text) = text.filter(|s| !s.trim().is_empty()) else {
            return Ok(json!({"ok":false,"chats":[]}));
        };
        let chats = send_text(state, targets, &text).await?;
        return Ok(outcome(chats));
    }
    let items = share_send::attachments(&state.db, &attachments, normalize)?;
    if items.is_empty() {
        return Ok(json!({"ok":false,"chats":[]}));
    }
    let images = images.unwrap_or_else(|| {
        items
            .iter()
            .all(|v| share_send::visual(v["fileName"].as_str().unwrap_or_default()))
    });
    let content =
        json!({"type":if images {"IMAGES"}else{"FILES"},"value":{"items":items}}).to_string();
    let mut chats = send_many(state, targets.clone(), content).await?;
    if let Some(caption) = caption.filter(|s| !s.trim().is_empty()) {
        chats.extend(send_text(state, targets, &caption).await?);
    }
    Ok(outcome(chats))
}
fn outcome(chats: Vec<DChat>) -> Value {
    let ok = !chats.is_empty()
        && chats
            .iter()
            .all(|row| row.status == plain_rs::chat::enums::ChatStatus::Sent);
    json!({"ok":ok,"chats":chats})
}
pub(crate) async fn share_content(state: &ServerState, id: &str) -> Result<Value> {
    let service = plain_rs::files::shares::Service::new(state.db.clone(), state.prefs.clone());
    let row = state
        .db
        .share_get(id)?
        .ok_or_else(|| anyhow::anyhow!("Share unavailable"))?;
    let actor = state
        .prefs
        .get::<String>(plain_rs::prefs::system::keys::CLIENT_ID)?
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Missing identity"))?;
    let facts = state
        .host
        .call("discoveryFacts", json!({}))
        .await
        .map_err(anyhow::Error::msg)?;
    let ip = facts["ips"]
        .as_array()
        .and_then(|v| v.first())
        .and_then(Value::as_str)
        .unwrap_or_default();
    share_send::folder_card(
        &service,
        &row,
        &actor,
        ip,
        state.prefs.get_user::<u16>("https_port")?.unwrap_or(8443),
    )
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
