use crate::assembly::server::ServerState;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use plain_rs::chat::enums::ChatStatus;
use plain_rs::chat::enums::PeerStatus;
use plain_rs::db::DChannel;
use plain_rs::db::DChat;
use plain_rs::db::DNearbyDeviceCache;
use plain_rs::db::DPeer;
use plain_rs::db::chat_store;
use plain_rs::db::chat_store::SaveMode;
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum Request {
    CreateChat {
        to_id: String,
        channel_id: String,
        content: String,
    },
    ReceiveChat {
        from_id: String,
        channel_id: String,
        content: String,
        signature: String,
        timestamp: i64,
    },
    ChatDelivery {
        id: String,
        results: Option<Vec<plain_rs::chat::channels::chat_helper::ChannelDeliveryResult>>,
        retry: bool,
    },
    Peers {
        statuses: Vec<PeerStatus>,
    },
    Peer {
        id: String,
    },
    PeersByIds {
        ids: Vec<String>,
    },
    PeerAddress {
        id: String,
        expected: DPeer,
    },
    PeerFileUrl {
        id: String,
        expected: DPeer,
        file_id: String,
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
        device_type: plain_rs::chat::enums::DeviceType,
    },
    PatchPeer {
        before: DPeer,
        after: DPeer,
    },
    PatchChannel {
        before: DChannel,
        after: DChannel,
    },
    StartPairing {
        target: plain_rs::peers::pairing::sessions::Target,
        device: crate::services::discovery::pairing::Device,
    },
    CompletePairing {
        response: plain_rs::peers::pairing::protocol::PairingResponse,
        sender_ip: String,
    },
    RespondPairing {
        request: plain_rs::peers::pairing::protocol::PairingRequest,
        accepted: bool,
        device: crate::services::discovery::pairing::Device,
    },
    CancelPairing {
        id: String,
        generation: Option<String>,
    },
    ExpirePairing {
        id: String,
        generation: String,
    },
    PairingTickets,
    ReceivePairingCancel {
        cancel: plain_rs::peers::pairing::protocol::PairingCancel,
    },
    ReceivePairingRequest {
        request: plain_rs::peers::pairing::protocol::PairingRequest,
    },
    SavePairedPeer {
        facts: plain_rs::peers::pairing::peer_store::Facts,
    },
    AuthenticatePeer {
        from_id: String,
        channel_id: String,
        body: String,
    },
    AuthenticateEnvelope {
        key: String,
        public_key: String,
        body: String,
    },
    PreparePeer {
        id: String,
        operation: crate::services::peers::peer_wire::Operation,
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
        message_type: plain_rs::chat::enums::ChannelSystemMessageType,
        payload: String,
    },
    PrepareChannel {
        channel: DChannel,
        message_type: plain_rs::chat::enums::ChannelSystemMessageType,
        target: String,
        name: String,
        device_type: plain_rs::chat::enums::DeviceType,
    },
    ReceiveChannel {
        actor: String,
        from_id: String,
        message_type: plain_rs::chat::enums::ChannelSystemMessageType,
        payload: String,
    },
    CreateChannel {
        actor: String,
        name: String,
    },
    ChannelAction {
        actor: String,
        id: String,
        operation: plain_rs::chat::channels::state::Action,
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
    LatestChats,
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
pub(crate) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if let Request::ReceiveChannel {
        from_id,
        message_type,
        payload,
        ..
    } = &request
    {
        return match crate::services::chat::channel_runtime::receive(
            &state,
            from_id,
            *message_type,
            payload,
        )
        .await
        {
            Ok(value) => Json(json!({"result":value})).into_response(),
            Err(error) => (
                StatusCode::BAD_REQUEST,
                Json(json!({"error":error.to_string()})),
            )
                .into_response(),
        };
    }
    let db = state.db.clone();
    let directory = state.directory.clone();
    let prefs = state.prefs.clone();
    let pairing = state.pairing.clone();
    let transport = state.transport.clone();
    let previews = state.previews.clone();
    let prewarmer = state.prewarmer.clone();
    let nearby_devices = state.nearby_devices.clone();
    let events = state.events.clone();
    let runtime = state.clone();
    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<Value> {
        use chat_store::{channels, messages, nearby, peers};
        Ok(match request {
            Request::CreateChat {
                to_id,
                channel_id,
                content,
            } => {
                let chat =
                    plain_rs::chat::messages::message_lifecycle::create(&db, &to_id, &channel_id, &content)?;
                previews.request(&chat.id);
                serde_json::to_value(chat)?
            }
            Request::ReceiveChat {
                from_id,
                channel_id,
                content,
                signature,
                timestamp,
            } => {
                let received = plain_rs::chat::messages::message_lifecycle::receive(
                    &db,
                    &from_id,
                    &channel_id,
                    &content,
                    &signature,
                    timestamp,
                )?;
                if let Some(received) = &received {
                    previews.request(&received.chat.id);
                }
                serde_json::to_value(received)?
            }
            Request::ChatDelivery { id, results, retry } => serde_json::to_value(
                plain_rs::chat::messages::message_lifecycle::delivery(&db, &id, results, retry)?,
            )?,
            Request::Peers { statuses } => {
                let rows = peers::all(&db)?;
                serde_json::to_value(
                    rows.iter()
                        .filter(|p| statuses.is_empty() || statuses.contains(&p.status))
                        .map(crate::services::peers::peer_address::view)
                        .collect::<Vec<_>>(),
                )?
            }
            Request::Peer { id } => serde_json::to_value(
                peers::get(&db, &id)?
                    .as_ref()
                    .map(crate::services::peers::peer_address::view),
            )?,
            Request::PeersByIds { ids } => {
                let rows = peers::all(&db)?;
                serde_json::to_value(
                    rows.iter()
                        .filter(|p| ids.contains(&p.id))
                        .map(crate::services::peers::peer_address::view)
                        .collect::<Vec<_>>(),
                )?
            }
            Request::PeerAddress { id, expected } => {
                serde_json::to_value(crate::services::peers::peer_address::address(
                    &crate::services::peers::peer_address::current(&db, &id, &expected)?,
                ))?
            }
            Request::PeerFileUrl {
                id,
                expected,
                file_id,
            } => json!(crate::services::peers::peer_address::file_url(
                &crate::services::peers::peer_address::current(&db, &id, &expected)?,
                &file_id
            )?),
            Request::SavePeers { items, mode } => {
                peers::save(&db, &items, mode)?;
                json!(true)
            }
            Request::DeletePeers { ids } => {
                let deleted = peers::delete(&db, &ids)?;
                for id in ids {
                    transport.forget(&id);
                    prewarmer.forget(&id);
                }
                json!(deleted)
            }
            Request::RemovePeer { id } => json!(
                crate::graphql::resolvers::peers::actions::remove(&runtime, &id)?
            ),
            Request::RemoveChannel { id } => serde_json::to_value(
                plain_rs::chat::attachments::app_file_store::chat_deletion::remove_channel(
                    &db, &directory, &id,
                )?,
            )?,
            Request::UnpairPeer { id } => json!(
                crate::graphql::resolvers::peers::actions::unpair(&runtime, &id)?
            ),
            Request::DiscoverPeer {
                id,
                ips,
                port,
                name,
                device_type,
            } => serde_json::to_value(
                peers::discovered(&db, &id, &ips, port, &name, device_type)?
                    .as_ref()
                    .map(crate::services::peers::peer_address::view),
            )?,
            Request::PatchPeer { before, after } => serde_json::to_value(
                peers::patch(&db, &before, &after)?
                    .as_ref()
                    .map(crate::services::peers::peer_address::view),
            )?,
            Request::PatchChannel { before, after } => {
                serde_json::to_value(channels::patch(&db, &before, &after)?)?
            }
            Request::StartPairing { target, device } => {
                crate::services::discovery::pairing::start(
                    &prefs, &pairing, target, device,
                )?
            }
            Request::CompletePairing {
                response,
                sender_ip,
            } => crate::services::discovery::pairing::complete(
                &db, &prefs, &pairing, response, &sender_ip,
            )?,
            Request::RespondPairing {
                request,
                accepted,
                device,
            } => crate::services::discovery::pairing::respond(
                &db, &prefs, &pairing, request, accepted, device,
            )?,
            Request::CancelPairing { id, generation } => {
                crate::services::discovery::pairing::cancel(
                    &prefs,
                    &pairing,
                    &id,
                    generation.as_deref(),
                )?
            }
            Request::ExpirePairing { id, generation } => {
                serde_json::to_value(pairing.expire(&id, &generation))?
            }
            Request::PairingTickets => serde_json::to_value(pairing.tickets())?,
            Request::ReceivePairingCancel { cancel } => {
                crate::services::discovery::pairing::receive_cancel(
                    &prefs, &pairing, cancel,
                )?
            }
            Request::ReceivePairingRequest { request } => {
                json!(
                    crate::services::discovery::pairing::receive_request(
                        &pairing, &request
                    )
                )
            }
            Request::SavePairedPeer { facts } => {
                serde_json::to_value(plain_rs::peers::pairing::peer_store::save(&db, facts)?)?
            }
            Request::AuthenticatePeer {
                from_id,
                channel_id,
                body,
            } => crate::services::peers::peer_wire::authenticate(
                &db,
                &from_id,
                &channel_id,
                &body,
            ),
            Request::AuthenticateEnvelope {
                key,
                public_key,
                body,
            } => crate::services::peers::peer_wire::envelope(&key, &public_key, &body),
            Request::PreparePeer { id, operation } => serde_json::to_value(
                crate::services::peers::peer_wire::prepare(&db, &prefs, &id, operation)?,
            )?,
            Request::EncryptPeer { key, body } => json!(
                crate::services::peers::peer_wire::encrypt(&key, &body)?
            ),
            Request::DecryptPeer { key, body } => json!(
                crate::services::peers::peer_wire::decrypt(&key, &body)?
            ),
            Request::SignChannel {
                message_type,
                payload,
            } => json!(crate::services::chat::channel_outgoing::wire(
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
            } => serde_json::to_value(crate::services::chat::channel_outgoing::prepare(
                &db,
                &prefs,
                &channel,
                message_type,
                &target,
                &name,
                device_type,
            )?)?,
            Request::ReceiveChannel { .. } => unreachable!("handled by channel runtime"),
            Request::CreateChannel { actor, name } => {
                serde_json::to_value(plain_rs::chat::channels::state::create(&db, &actor, &name)?)?
            }
            Request::ChannelAction {
                actor,
                id,
                operation,
            } => serde_json::to_value(plain_rs::chat::channels::state::apply(
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
            Request::LatestChats => {
                serde_json::to_value(plain_rs::chat::cacher::ChatCacher::snapshot(&db)?)?
            }
            Request::Chat { id } => serde_json::to_value(messages::get(&db, &id)?)?,
            Request::SaveChats { items, mode } => {
                messages::save(&db, &items, mode)?;
                json!(true)
            }
            Request::DeleteChats { ids } => {
                json!(plain_rs::chat::attachments::app_file_store::chat_deletion::delete(
                    &db,
                    &directory,
                    plain_rs::chat::attachments::app_file_store::chat_deletion::Selection::Ids(&ids)
                )?)
            }
            Request::DeletePeerChats { id } => {
                json!(plain_rs::chat::attachments::app_file_store::chat_deletion::delete(
                    &db,
                    &directory,
                    plain_rs::chat::attachments::app_file_store::chat_deletion::Selection::Peer(&id)
                )?)
            }
            Request::DeleteChannelChats { id } => {
                json!(plain_rs::chat::attachments::app_file_store::chat_deletion::delete(
                    &db,
                    &directory,
                    plain_rs::chat::attachments::app_file_store::chat_deletion::Selection::Channel(&id)
                )?)
            }
            Request::ChatStatus { id, status, data } => {
                json!(messages::status(&db, &id, status, data.as_deref())?)
            }
            Request::ChatContent { id, content } => json!(messages::content(&db, &id, &content)?),
            Request::ChatIds { query } => json!(messages::ids(&db, &query)?),
            Request::Nearby => {
                let interfaces = plain_rs::transport::lan::local_interfaces();
                let mut rows = serde_json::to_value(nearby::all(&db)?)?;
                for row in rows.as_array_mut().unwrap() {
                    let ips: Vec<String> = serde_json::from_value(row["ips"].clone())?;
                    row["bestIp"] = json!(plain_rs::transport::lan::best(&ips, &interfaces));
                }
                rows
            }
            Request::SaveNearby { item } => {
                nearby::save(&db, &item)?;
                json!(true)
            }
            Request::TouchNearby { id } => json!(nearby::touch(&db, &id)?),
            Request::DeleteNearby { id } => {
                let deleted = nearby::delete(&db, &id)?;
                if nearby_devices.forget(&id) {
                    let _ = events.send(plain_rs::events::WsEvent::broadcast(
                        plain_rs::chat::events::WS_NEARBY_DEVICE_FOUND,
                        json!({"revision":nearby_devices.snapshot().revision,"eventId":null})
                            .to_string(),
                    ));
                }
                json!(deleted)
            }
        })
    })
    .await;
    match result {Ok(Ok(value))=>Json(json!({"result":value})).into_response(),error=>(StatusCode::BAD_REQUEST,Json(json!({"error": match error {Ok(Err(e))=>e.to_string(),Err(e)=>e.to_string(),_=>unreachable!()}}))).into_response()}
}
#[cfg(test)]
#[path = "../../../tests/unit/services/chat/chat_store_routes.rs"]
mod tests;
