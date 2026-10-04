use super::{peer_query::Query, server::ServerState};
use crate::{
    chat::{
        enums::{ChannelSystemMessageType, ChatStatus},
        peer_auth::AuthenticatedPeer,
    },
    content_types::Instant,
    db::DChat,
};
use async_graphql::{Context, EmptySubscription, ID, Object, Schema, SimpleObject};
use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(SimpleObject)]
struct ChatItem {
    from_id: ID,
    to_id: ID,
    channel_id: Option<ID>,
    id: ID,
    content: String,
    created_at: Instant,
    updated_at: Instant,
    status: ChatStatus,
    status_data: String,
}
impl TryFrom<DChat> for ChatItem {
    type Error = anyhow::Error;
    fn try_from(row: DChat) -> anyhow::Result<Self> {
        Ok(Self {
            from_id: ID(row.from_id),
            to_id: ID(row.to_id),
            channel_id: (!row.channel_id.is_empty()).then_some(ID(row.channel_id)),
            id: ID(row.id),
            content: row.content,
            created_at: Instant(chrono::DateTime::parse_from_rfc3339(&row.created_at)?.into()),
            updated_at: Instant(chrono::DateTime::parse_from_rfc3339(&row.updated_at)?.into()),
            status: row.status,
            status_data: row.status_data,
        })
    }
}
struct PeerContext {
    state: ServerState,
    authenticated: AuthenticatedPeer,
    channel_id: String,
}
pub(super) struct Mutation;
#[Object]
impl Mutation {
    async fn create_chat_item(
        &self,
        ctx: &Context<'_>,
        content: String,
    ) -> async_graphql::Result<Vec<ChatItem>> {
        let c = ctx.data_unchecked::<PeerContext>();
        let Some(received) = crate::chat::message_lifecycle::receive(
            &c.state.db,
            &c.authenticated.peer.id,
            &c.channel_id,
            &content,
            &c.authenticated.signature_b64,
            c.authenticated.timestamp,
        )?
        else {
            return Ok(vec![]);
        };
        let item = ChatItem::try_from(received.chat.clone())?;
        let _ = c.state.events.send(crate::ws_event::WsEvent::broadcast(
            crate::chat::events::WS_MESSAGE_CREATED,
            json!([{"id":received.chat.id}]).to_string(),
        ));
        if let Ok(content) = serde_json::from_str::<Value>(&received.chat.content) {
            if matches!(content["type"].as_str(), Some("FILES" | "IMAGES")) {
                if let Some(files) = content["value"]["items"].as_array() {
                    for file in files {
                        if let Some(id) = file["id"].as_str() {
                            if let Err(error) =
                                c.state
                                    .downloads
                                    .enqueue(&received.chat.id, id, &received.peer.id)
                            {
                                log::warn!("Incoming attachment queue: {error}");
                            }
                        }
                    }
                }
            }
        }
        c.state.previews.request(&received.chat.id);
        Ok(vec![item])
    }
    async fn channel_system_message(
        &self,
        ctx: &Context<'_>,
        r#type: ChannelSystemMessageType,
        payload: String,
    ) -> async_graphql::Result<bool> {
        let c = ctx.data_unchecked::<PeerContext>();
        let actor = c
            .state
            .prefs
            .get::<String>("client_id")?
            .filter(|id| !id.is_empty())
            .ok_or_else(|| async_graphql::Error::new("Missing client identity"))?;
        let received = match crate::chat::channel::incoming::receive(
            &c.state.db,
            &actor,
            &c.authenticated.peer.id,
            r#type,
            &payload,
        ) {
            Ok(received) => received,
            Err(error) => {
                log::warn!("Channel message rejected: {error}");
                return Ok(false);
            }
        };
        if received.changed || received.invite.is_some() || received.cancel.is_some() {
            let _ = c.state.events.send(crate::ws_event::WsEvent::broadcast(
                crate::chat::events::WS_CHANNELS_UPDATED,
                serde_json::to_string(&received)?,
            ));
        }
        if received.broadcast {
            if let Some(channel) = received.channel.clone() {
                let state = c.state.clone();
                tokio::spawn(async move {
                    let mut stop = state.stop.clone();
                    if *stop.borrow() {
                        return;
                    }
                    tokio::select! {
                        _ = stop.changed() => {},
                        result = broadcast(&state, &channel) => {
                            if let Err(error) = result { log::warn!("Channel update broadcast: {error}"); }
                        },
                    }
                });
            }
        }
        Ok(received.accepted)
    }
    async fn start_aware(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        let c = ctx.data_unchecked::<PeerContext>();
        // Re-read the peer before handing the OS subscription to Native.
        let peer = crate::db::chat_store::peers::get(&c.state.db, &c.authenticated.peer.id)?
            .ok_or_else(|| async_graphql::Error::new("Unknown peer"))?;
        Ok(c.state
            .host
            .call("peerStartAware", json!({"peer":peer}))
            .await
            .map_err(async_graphql::Error::new)?
            .as_bool()
            .unwrap_or(false))
    }
}
async fn broadcast(state: &ServerState, channel: &crate::db::DChannel) -> anyhow::Result<()> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Device {
        name: String,
        device_type: crate::chat::enums::DeviceType,
    }
    let device: Device = serde_json::from_value(
        state
            .host
            .call("peerDeviceInfo", json!({}))
            .await
            .map_err(anyhow::Error::msg)?,
    )?;
    let Some(channel) = crate::db::chat_store::channels::get(&state.db, &channel.id)? else {
        return Ok(());
    };
    let prepared = super::channel_outgoing::prepare(
        &state.db,
        &state.prefs,
        &channel,
        ChannelSystemMessageType::Update,
        "",
        &device.name,
        device.device_type,
    )?;
    let wire =
        super::channel_outgoing::wire(&state.prefs, prepared.message_type, &prepared.payload)?;
    for target in prepared.targets {
        match super::peer_transport::send(
            &state.host,
            &state.transport,
            &target.peer,
            &target.channel_id,
            &crate::base64_decode(&target.key),
            &wire,
        )
        .await
        {
            Ok(response)
                if response["data"]["channelSystemMessage"].as_bool() == Some(true)
                    && response
                        .get("errors")
                        .is_none_or(|v| v.is_null() || v.as_array().is_some_and(Vec::is_empty)) => {
            }
            Ok(_) => log::warn!("Channel update rejected by {}", target.peer.id),
            Err(error) => log::warn!("Channel update to {}: {error}", target.peer.id),
        }
    }
    Ok(())
}
pub(super) type PeerSchema = Schema<Query, Mutation, EmptySubscription>;
pub(super) fn schema() -> PeerSchema {
    Schema::build(Query, Mutation, EmptySubscription)
        .register_output_type::<crate::content_types::Long>()
        .register_output_type::<crate::chat::enums::PeerStatus>()
        .limit_depth(16)
        .limit_complexity(256)
        .finish()
}
pub(super) async fn execute(
    state: &ServerState,
    client_id: &str,
    channel_id: &str,
    body: &[u8],
) -> (u16, Vec<u8>) {
    if !state.prefs.get_user_or("service", false) {
        return (403, vec![]);
    }
    let authenticated =
        match crate::chat::peer_auth::authenticate(&state.db, client_id, channel_id, body) {
            Ok(authenticated) => authenticated,
            Err(error) => return (error.http_status(), vec![]),
        };
    let request = match serde_json::from_str::<async_graphql::Request>(&authenticated.graphql_json)
    {
        Ok(request) => request,
        Err(_) => return (400, vec![]),
    };
    let key = authenticated.key.clone();
    let response = state
        .peer_schema
        .execute(request.data(PeerContext {
            state: state.clone(),
            authenticated,
            channel_id: channel_id.into(),
        }))
        .await;
    match serde_json::to_vec(&response) {
        Ok(body) => crate::xchacha_encrypt_raw(&key, &body)
            .map(|body| (200, body))
            .unwrap_or((500, vec![])),
        Err(_) => (500, vec![]),
    }
}
pub(super) async fn public(
    State(state): State<ServerState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let id = headers
        .get("c-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let cid = headers
        .get("c-cid")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let (status, body) = execute(&state, id, cid, &body).await;
    (
        StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        [("content-type", "application/octet-stream")],
        body,
    )
        .into_response()
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Request {
    client_id: String,
    channel_id: String,
    body: String,
}
pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let (status, body) = execute(
        &state,
        &request.client_id,
        &request.channel_id,
        &crate::base64_decode(&request.body),
    )
    .await;
    Json(json!({"result":{"status":status,"body":crate::base64_encode(&body)}})).into_response()
}
#[cfg(test)]
#[path = "../../tests/unit/content_api/peer_graphql.rs"]
mod tests;
