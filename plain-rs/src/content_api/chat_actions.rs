use super::server::ServerState;
use crate::{
    chat::{events, message_commands, share_send},
    db::DChat,
    ws_event::WsEvent,
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
pub(super) fn committed(state: &ServerState, chat: &DChat, event: i32) {
    state.previews.request(&chat.id);
    let _ = state.events.send(WsEvent::broadcast(
        event,
        json!([crate::chat::service::chat_to_json(chat)]).to_string(),
    ));
}
pub(super) async fn deliver(state: &ServerState, row: DChat) -> Result<DChat> {
    if row.channel_id.is_empty() && (row.to_id.is_empty() || row.to_id == "local") {
        return Ok(row);
    }
    super::chat_delivery::deliver(state, &row.id, None)
        .await?
        .chat
        .ok_or_else(|| anyhow::anyhow!("Message removed"))
}
pub(super) async fn send(state: &ServerState, target: &str, content: &str) -> Result<DChat> {
    let (to, channel) = message_commands::target(target)?;
    let row = crate::chat::message_lifecycle::create(&state.db, &to, &channel, content)?;
    committed(state, &row, events::WS_MESSAGE_CREATED);
    deliver(state, row).await
}
pub(super) async fn send_many(
    state: &ServerState,
    targets: Vec<String>,
    content: String,
) -> Result<Vec<DChat>> {
    ensure!(targets.len() <= 128, "Too many share targets");
    let targets = targets
        .iter()
        .map(|target| message_commands::target(target))
        .collect::<Result<Vec<_>>>()?;
    let rows = crate::chat::message_lifecycle::create_many(&state.db, &targets, &content)?;
    for row in &rows {
        committed(state, row, events::WS_MESSAGE_CREATED);
    }
    futures_util::future::join_all(rows.into_iter().map(|row| deliver(state, row)))
        .await
        .into_iter()
        .collect()
}

pub(super) async fn send_text(
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
        ids: crate::chat::app_file_store::content_refs::collect(&content),
    };
    send_many(state, targets, content.to_string()).await
}
pub(super) async fn share(
    state: &ServerState,
    targets: Vec<String>,
    uris: Vec<String>,
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
    if uris.is_empty() {
        let Some(text) = text.filter(|s| !s.trim().is_empty()) else {
            return Ok(json!({"ok":false,"chats":[]}));
        };
        let chats = send_text(state, targets, &text).await?;
        return Ok(outcome(chats));
    }
    let mut unique = std::collections::HashSet::new();
    let uris: Vec<_> = uris
        .into_iter()
        .filter(|uri| unique.insert(uri.clone()))
        .collect();
    let facts = state
        .host
        .call("chatPickedFacts", json!({"uris":uris}))
        .await
        .map_err(anyhow::Error::msg)?;
    let facts: Vec<share_send::PickedFile> = serde_json::from_value(facts)?;
    let mut items = share_send::placeholders(&facts, normalize)?;
    if items.is_empty() {
        return Ok(json!({"ok":false,"chats":[]}));
    }
    let images = images.unwrap_or_else(|| {
        items
            .iter()
            .all(|v| share_send::visual(v["fileName"].as_str().unwrap_or_default()))
    });
    let target_pairs = targets
        .iter()
        .map(|target| message_commands::target(target))
        .collect::<Result<Vec<_>>>()?;
    let content =
        json!({"type":if images {"IMAGES"}else{"FILES"},"value":{"items":items}}).to_string();
    let rows = crate::chat::message_lifecycle::create_many(&state.db, &target_pairs, &content)?;
    for row in &rows {
        committed(state, row, events::WS_MESSAGE_CREATED);
    }
    let mut imports = Imported {
        db: state.db.clone(),
        directory: state.directory.clone(),
        ids: Default::default(),
    };
    let staging = state.directory.join("chat-picks");
    std::fs::create_dir_all(&staging)?;
    let mut warnings = vec![];
    for (item, fact) in items.iter_mut().zip(&facts) {
        let path = staging.join(crate::utils::short_uuid::short_uuid());
        let mut stage = Stage {
            host: state.host.clone(),
            path: path.clone(),
            released: false,
        };
        let name = item["fileName"].as_str().unwrap_or_default();
        let kind = if share_send::image(name) {
            "image"
        } else if share_send::visual(name) {
            "video"
        } else {
            "file"
        };
        let result = state
            .host
            .call_wait(
                "chatPickedRead",
                json!({"uri":fact.uri,"path":path,"kind":kind}),
            )
            .await;
        if let Ok(metadata) = result {
            let imported = crate::chat::app_file_store::import_file(
                &state.db,
                &state.directory,
                &path,
                item["fileName"].as_str().unwrap_or_default(),
                &fact.mime_type,
            );
            if let Ok(imported) = imported {
                *imports.ids.entry(imported.id.clone()).or_default() += 1;
                item["uri"] = json!(format!("fid:{}", imported.fid_suffix));
                item["size"] = json!(std::fs::metadata(&imported.real_path)?.len());
                for key in ["width", "height", "durationMs"] {
                    item[key] = metadata[key].clone();
                }
            } else if let Err(error) = imported {
                warnings.push(error.to_string());
            }
        } else if let Err(error) = result {
            warnings.push(error);
        }
        stage.release().await?;
    }
    let ids: Vec<_> = rows.into_iter().map(|row| row.id).collect();
    let rows = message_commands::replace_many(&state.db, &state.directory, &ids, items)?;
    let mut chats = futures_util::future::join_all(rows.into_iter().flatten().map(|row| async {
        committed(state, &row, events::WS_MESSAGE_UPDATED);
        deliver(state, row).await
    }))
    .await
    .into_iter()
    .collect::<Result<Vec<_>>>()?;
    let all_present = chats.len() == ids.len();
    if let Some(caption) = caption.filter(|s| !s.trim().is_empty()) {
        chats.extend(send_text(state, targets, &caption).await?);
    }
    let mut result = outcome(chats);
    result["warnings"] = json!(warnings);
    if !all_present {
        result["ok"] = json!(false);
    }
    Ok(result)
}
fn outcome(chats: Vec<DChat>) -> Value {
    let ok = !chats.is_empty()
        && chats
            .iter()
            .all(|row| row.status == crate::chat::enums::ChatStatus::Sent);
    json!({"ok":ok,"chats":chats})
}
pub(super) async fn share_content(state: &ServerState, id: &str) -> Result<Value> {
    let service = crate::shares::Service::new(state.db.clone(), state.prefs.clone());
    let row = state
        .db
        .share_get(id)?
        .ok_or_else(|| anyhow::anyhow!("Share unavailable"))?;
    let actor = state
        .prefs
        .get::<String>("client_id")?
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
pub(super) fn delete_query(state: &ServerState, query: &str) -> Result<usize> {
    let ids = if query.trim().is_empty() {
        vec![]
    } else {
        crate::db::chat_store::messages::ids(&state.db, query)?
    };
    delete(state, ids)
}

pub(super) fn delete(state: &ServerState, ids: Vec<String>) -> Result<usize> {
    let count = crate::chat::app_file_store::chat_deletion::delete(
        &state.db,
        &state.directory,
        crate::chat::app_file_store::chat_deletion::Selection::Ids(&ids),
    )?;
    let _ = state.events.send(WsEvent::broadcast(
        events::WS_MESSAGE_DELETED,
        json!(format!("ids={}", ids.join(","))).to_string(),
    ));
    Ok(count)
}

struct Imported {
    db: std::sync::Arc<crate::db::Db>,
    directory: std::path::PathBuf,
    ids: std::collections::BTreeMap<String, i64>,
}
impl Drop for Imported {
    fn drop(&mut self) {
        if let Err(error) = crate::chat::app_file_store::content_refs::release_unbound(
            &self.db,
            &self.directory,
            std::mem::take(&mut self.ids),
        ) {
            log::warn!("Unable to release unused chat imports: {error}");
        }
    }
}
struct Stage {
    host: std::sync::Arc<super::host::Host>,
    path: std::path::PathBuf,
    released: bool,
}
impl Stage {
    async fn release(&mut self) -> Result<()> {
        let receipt = self
            .host
            .call("chatPickedRelease", json!({"path":self.path}))
            .await
            .map_err(anyhow::Error::msg)?;
        ensure!(receipt == json!(true), "Selected file release rejected");
        self.released = true;
        Ok(())
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        if self.released {
            return;
        }
        let host = self.host.clone();
        let path = self.path.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = host.call("chatPickedRelease", json!({"path":path})).await;
            });
        }
    }
}

pub(super) async fn retry(state: &ServerState, id: String) -> Result<DChat> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let sender = std::sync::Arc::new(std::sync::Mutex::new(Some(sender)));
    let state = state.clone();
    tokio::spawn(async move {
        let mut stop = state.stop.clone();
        let result = tokio::select! {
            _=stop.changed()=>Err(anyhow::anyhow!("Chat service stopped")),
            result=super::chat_delivery::deliver_observed(&state,&id,None,|chat| {
                if let Some(sender)=sender.lock().unwrap().take() {let _=sender.send(Ok(chat.clone()));}
            })=>result.and_then(|receipt|receipt.chat.ok_or_else(||anyhow::anyhow!("Message unavailable"))),
        };
        if let Some(sender) = sender.lock().unwrap().take() {
            let _ = sender.send(result);
        }
    });
    receiver.await?
}
