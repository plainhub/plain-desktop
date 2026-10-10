use crate::adapters::mobile::{peers::peer_query::Query, runtime::server::ServerState};
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
use {
    crate::graphql::types::Instant,
    plain_rs::{
        chat::{
            enums::{ChannelSystemMessageType, ChatStatus},
            peer_auth::AuthenticatedPeer,
        },
        db::DChat,
    },
};

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
pub(in crate::adapters::mobile) struct Mutation;
#[Object]
impl Mutation {
    async fn create_chat_item(
        &self,
        ctx: &Context<'_>,
        content: String,
    ) -> async_graphql::Result<Vec<ChatItem>> {
        let c = ctx.data_unchecked::<PeerContext>();
        let Some(received) = plain_rs::chat::message_lifecycle::receive(
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
        let _ = c.state.events.send(plain_rs::ws_event::WsEvent::broadcast(
            plain_rs::chat::events::WS_MESSAGE_CREATED,
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
        match crate::adapters::mobile::chat::channel_runtime::receive(
            &c.state,
            &c.authenticated.peer.id,
            r#type,
            &payload,
        )
        .await
        {
            Ok(received) => Ok(received["accepted"].as_bool().unwrap_or(false)),
            Err(error) => {
                log::warn!("Channel message rejected: {error}");
                Ok(false)
            }
        }
    }
    async fn start_aware(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        let c = ctx.data_unchecked::<PeerContext>();
        // Re-read the peer before handing the OS subscription to Native.
        let peer = plain_rs::db::chat_store::peers::get(&c.state.db, &c.authenticated.peer.id)?
            .ok_or_else(|| async_graphql::Error::new("Unknown peer"))?;
        Ok(c.state
            .host
            .call(
                "peerStartAware",
                json!({"peer":crate::adapters::mobile::peers::peer_address::view(&peer)}),
            )
            .await
            .map_err(async_graphql::Error::new)?
            .as_bool()
            .unwrap_or(false))
    }
}
pub(in crate::adapters::mobile) type PeerSchema = Schema<Query, Mutation, EmptySubscription>;
pub(in crate::adapters::mobile) fn schema() -> PeerSchema {
    Schema::build(Query, Mutation, EmptySubscription)
        .register_output_type::<crate::graphql::types::Long>()
        .register_output_type::<plain_rs::chat::enums::PeerStatus>()
        .limit_depth(16)
        .limit_complexity(256)
        .finish()
}
pub(in crate::adapters::mobile) async fn execute(
    state: &ServerState,
    client_id: &str,
    channel_id: &str,
    body: &[u8],
) -> (u16, Vec<u8>) {
    let authenticated =
        match plain_rs::chat::peer_auth::authenticate(&state.db, client_id, channel_id, body) {
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
        Ok(body) => plain_rs::crypto::xchacha_encrypt_raw(&key, &body)
            .map(|body| (200, body))
            .unwrap_or((500, vec![])),
        Err(_) => (500, vec![]),
    }
}
pub(in crate::adapters::mobile) async fn public(
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
pub(in crate::adapters::mobile) struct Request {
    client_id: String,
    channel_id: String,
    body: String,
}
pub(in crate::adapters::mobile) async fn call(
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
        &plain_rs::utils::base64::base64_decode(&request.body),
    )
    .await;
    Json(json!({"result":{"status":status,"body":plain_rs::utils::base64::base64_encode(&body)}}))
        .into_response()
}
#[cfg(test)]
#[path = "../../../../tests/unit/adapters/mobile/peers/peer_graphql.rs"]
mod tests;
