use super::server::ServerState;
use crate::{
    chat::enums::{ChatStatus, PeerStatus},
    db::{
        DChannel, DChat, DNearbyDeviceCache, DPeer,
        chat_store::{self, SaveMode},
    },
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Request {
    Peers {
        statuses: Vec<PeerStatus>,
    },
    Peer {
        id: String,
    },
    PeersByIds {
        ids: Vec<String>,
    },
    SavePeers {
        items: Vec<DPeer>,
        mode: SaveMode,
    },
    DeletePeers {
        ids: Vec<String>,
    },
    RemovePeer {
        id: String,
    },
    RemoveChannel {
        id: String,
    },
    UnpairPeer {
        id: String,
    },
    DiscoverPeer {
        id: String,
        ips: Vec<String>,
        port: u16,
        name: String,
        device_type: crate::chat::enums::DeviceType,
    },
    PatchPeer {
        before: DPeer,
        after: DPeer,
    },
    PatchChannel {
        before: DChannel,
        after: DChannel,
    },
    PreparePeer {
        id: String,
        operation: super::peer_wire::Operation,
    },
    EncryptPeer {
        key: String,
        body: String,
    },
    DecryptPeer {
        key: String,
        body: String,
    },
    SignChannel {
        message_type: crate::chat::enums::ChannelSystemMessageType,
        payload: String,
    },
    PrepareChannel {
        channel: DChannel,
        message_type: crate::chat::enums::ChannelSystemMessageType,
        target: String,
        name: String,
        device_type: crate::chat::enums::DeviceType,
    },
    ReceiveChannel {
        actor: String,
        from_id: String,
        message_type: crate::chat::enums::ChannelSystemMessageType,
        payload: String,
    },
    CreateChannel {
        actor: String,
        name: String,
    },
    ChannelAction {
        actor: String,
        id: String,
        operation: crate::chat::channel::state::Action,
    },
    Channels,
    Channel {
        id: String,
    },
    SaveChannels {
        items: Vec<DChannel>,
        mode: SaveMode,
    },
    DeleteChannels {
        ids: Vec<String>,
    },
    Chats {
        filter: chat_store::messages::Filter,
    },
    Chat {
        id: String,
    },
    SaveChats {
        items: Vec<DChat>,
        mode: SaveMode,
    },
    DeleteChats {
        ids: Vec<String>,
    },
    DeletePeerChats {
        id: String,
    },
    DeleteChannelChats {
        id: String,
    },
    ChatStatus {
        id: String,
        status: ChatStatus,
        data: Option<String>,
    },
    ChatContent {
        id: String,
        content: String,
    },
    ChatIds {
        query: String,
    },
    Nearby,
    SaveNearby {
        item: DNearbyDeviceCache,
    },
    TouchNearby {
        id: String,
    },
    DeleteNearby {
        id: String,
    },
}
pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let db = state.db.clone();
    let directory = state.directory.clone();
    let prefs = state.prefs.clone();
    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<Value> {
        use chat_store::{channels, messages, nearby, peers};
        Ok(match request {
            Request::Peers { statuses } => serde_json::to_value(
                peers::all(&db)?
                    .into_iter()
                    .filter(|p| statuses.is_empty() || statuses.contains(&p.status))
                    .collect::<Vec<_>>(),
            )?,
            Request::Peer { id } => serde_json::to_value(peers::get(&db, &id)?)?,
            Request::PeersByIds { ids } => serde_json::to_value(
                peers::all(&db)?
                    .into_iter()
                    .filter(|p| ids.contains(&p.id))
                    .collect::<Vec<_>>(),
            )?,
            Request::SavePeers { items, mode } => {
                peers::save(&db, &items, mode)?;
                json!(true)
            }
            Request::DeletePeers { ids } => json!(peers::delete(&db, &ids)?),
            Request::RemovePeer { id } => json!(
                crate::chat::app_file_store::chat_deletion::delete(
                    &db,
                    &directory,
                    crate::chat::app_file_store::chat_deletion::Selection::PeerRecord(&id)
                )? != 0
            ),
            Request::RemoveChannel { id } => serde_json::to_value(
                crate::chat::app_file_store::chat_deletion::remove_channel(&db, &directory, &id)?,
            )?,
            Request::UnpairPeer { id } => json!(peers::unpair(&db, &id)?),
            Request::DiscoverPeer {
                id,
                ips,
                port,
                name,
                device_type,
            } => {
                serde_json::to_value(peers::discovered(&db, &id, &ips, port, &name, device_type)?)?
            }
            Request::PatchPeer { before, after } => {
                serde_json::to_value(peers::patch(&db, &before, &after)?)?
            }
            Request::PatchChannel { before, after } => {
                serde_json::to_value(channels::patch(&db, &before, &after)?)?
            }
            Request::PreparePeer { id, operation } => {
                serde_json::to_value(super::peer_wire::prepare(&db, &prefs, &id, operation)?)?
            }
            Request::EncryptPeer { key, body } => json!(super::peer_wire::encrypt(&key, &body)?),
            Request::DecryptPeer { key, body } => json!(super::peer_wire::decrypt(&key, &body)?),
            Request::SignChannel {
                message_type,
                payload,
            } => json!(super::channel_outgoing::wire(
                &prefs,
                message_type,
                &payload
            )?),
            Request::PrepareChannel {
                channel,
                message_type,
                target,
                name,
                device_type,
            } => serde_json::to_value(super::channel_outgoing::prepare(
                &db,
                &prefs,
                &channel,
                message_type,
                &target,
                &name,
                device_type,
            )?)?,
            Request::ReceiveChannel {
                actor,
                from_id,
                message_type,
                payload,
            } => serde_json::to_value(crate::chat::channel::incoming::receive(
                &db,
                &actor,
                &from_id,
                message_type,
                &payload,
            )?)?,
            Request::CreateChannel { actor, name } => {
                serde_json::to_value(crate::chat::channel::state::create(&db, &actor, &name)?)?
            }
            Request::ChannelAction {
                actor,
                id,
                operation,
            } => serde_json::to_value(crate::chat::channel::state::apply(
                &db, &actor, &id, operation,
            )?)?,
            Request::Channels => serde_json::to_value(channels::all(&db)?)?,
            Request::Channel { id } => serde_json::to_value(channels::get(&db, &id)?)?,
            Request::SaveChannels { items, mode } => {
                channels::save(&db, &items, mode)?;
                json!(true)
            }
            Request::DeleteChannels { ids } => json!(channels::delete(&db, &ids)?),
            Request::Chats { filter } => messages::list(&db, &filter)?,
            Request::Chat { id } => serde_json::to_value(messages::get(&db, &id)?)?,
            Request::SaveChats { items, mode } => {
                messages::save(&db, &items, mode)?;
                json!(true)
            }
            Request::DeleteChats { ids } => {
                json!(crate::chat::app_file_store::chat_deletion::delete(
                    &db,
                    &directory,
                    crate::chat::app_file_store::chat_deletion::Selection::Ids(&ids)
                )?)
            }
            Request::DeletePeerChats { id } => {
                json!(crate::chat::app_file_store::chat_deletion::delete(
                    &db,
                    &directory,
                    crate::chat::app_file_store::chat_deletion::Selection::Peer(&id)
                )?)
            }
            Request::DeleteChannelChats { id } => {
                json!(crate::chat::app_file_store::chat_deletion::delete(
                    &db,
                    &directory,
                    crate::chat::app_file_store::chat_deletion::Selection::Channel(&id)
                )?)
            }
            Request::ChatStatus { id, status, data } => {
                json!(messages::status(&db, &id, status, data.as_deref())?)
            }
            Request::ChatContent { id, content } => json!(messages::content(&db, &id, &content)?),
            Request::ChatIds { query } => json!(messages::ids(&db, &query)?),
            Request::Nearby => serde_json::to_value(nearby::all(&db)?)?,
            Request::SaveNearby { item } => {
                nearby::save(&db, &item)?;
                json!(true)
            }
            Request::TouchNearby { id } => json!(nearby::touch(&db, &id)?),
            Request::DeleteNearby { id } => json!(nearby::delete(&db, &id)?),
        })
    })
    .await;
    match result {Ok(Ok(value))=>Json(json!({"result":value})).into_response(),error=>(StatusCode::BAD_REQUEST,Json(json!({"error": match error {Ok(Err(e))=>e.to_string(),Err(e)=>e.to_string(),_=>unreachable!()}}))).into_response()}
}
#[cfg(test)]
#[path = "../../tests/unit/content_api/chat_store_routes.rs"]
mod tests;
