use super::{
    ble_wire::{self, Request},
    server::ServerState,
};
use anyhow::{Result, ensure};
use axum::{
    body::Body,
    extract::{
        Path, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::{collections::HashMap, sync::Mutex, time::Duration};
use tokio::sync::oneshot;

struct Exchange {
    generation: u64,
    created: std::time::Instant,
    bytes: Vec<u8>,
    reply: oneshot::Sender<Result<Vec<u8>, String>>,
}
#[derive(Default)]
pub(super) struct Runtime {
    pending: Mutex<HashMap<String, Exchange>>,
}
struct Lease<'a>(&'a Runtime, String);
impl Drop for Lease<'_> {
    fn drop(&mut self) {
        self.0.pending.lock().unwrap().remove(&self.1);
    }
}
impl Runtime {
    pub async fn exchange(
        &self,
        state: &ServerState,
        peer: &crate::db::DPeer,
        bytes: Vec<u8>,
    ) -> Result<Vec<u8>> {
        let generation = state
            .host
            .generation()
            .ok_or_else(|| anyhow::anyhow!("BLE host unavailable"))?;
        let token = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().unwrap();
            ensure!(pending.len() < 32, "BLE exchange capacity exceeded");
            pending.insert(
                token.clone(),
                Exchange {
                    generation,
                    created: std::time::Instant::now(),
                    bytes,
                    reply: tx,
                },
            );
        }
        let _lease = Lease(self, token.clone());
        let host = state.host.call_wait("peerTransportBleExchange", serde_json::json!({"token":token,"shortId":crate::chat::nearby_wire::short_id(&peer.id),"peer":super::peer_address::view(peer)}));
        tokio::pin!(host);
        let wait = async {
            let response = async { rx.await?.map_err(anyhow::Error::msg) };
            let host = async { host.await.map_err(anyhow::Error::msg) };
            let (_, bytes) = tokio::try_join!(host, response)?;
            Ok(bytes)
        };
        let mut stop = state.stop.clone();
        tokio::select! {
            result=tokio::time::timeout(Duration::from_secs(120), wait)=>result?,
            _=stop.changed()=>Err(anyhow::anyhow!("BLE core stopped")),
        }
    }
}
pub(super) async fn outgoing(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Path(token): Path<String>,
    ws: WebSocketUpgrade,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(mut exchange) = state.ble_transport.pending.lock().unwrap().remove(&token) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !state.host.is_current(exchange.generation)
        || exchange.created.elapsed() >= Duration::from_secs(30)
    {
        return StatusCode::GONE.into_response();
    }
    ws.max_message_size(ble_wire::MAX_MESSAGE).max_frame_size(ble_wire::MAX_MESSAGE).on_upgrade(move |socket| async move {
        let result = async {
            let mut socket = socket;
            socket.send(Message::Binary(exchange.bytes.into())).await.map_err(|e| e.to_string())?;
            let bytes = tokio::select! {
                _ = exchange.reply.closed() => return Err("BLE exchange canceled".into()),
                result = receive(&mut socket, Duration::from_secs(120)) => result.map_err(|e| e.to_string())?,
            };
            ble_wire::decode_response(&bytes).map_err(|e| e.to_string())?;
            Ok(bytes)
        }.await;
        let _ = exchange.reply.send(result);
    })
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Incoming {
    remote_host: String,
    characteristic_uuid: String,
}
pub(super) async fn incoming(
    State(state): State<ServerState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    ws.max_message_size(ble_wire::MAX_MESSAGE).max_frame_size(ble_wire::MAX_MESSAGE).on_upgrade(move |mut socket| async move {
        let _ = tokio::time::timeout(Duration::from_secs(120), async {
            let first = tokio::time::timeout(Duration::from_secs(15), socket.recv()).await?.ok_or_else(|| anyhow::anyhow!("BLE channel closed"))??;
            let Message::Text(text) = first else { anyhow::bail!("Missing BLE connection facts"); };
            ensure!(text.len() <= 4096, "BLE facts exceed limit");
            let facts: Incoming = serde_json::from_str(&text)?;
            ensure!(!facts.remote_host.is_empty() && facts.characteristic_uuid.eq_ignore_ascii_case("d8d5c4a0-8f0a-4e7d-b9e1-706c70616913"), "Invalid BLE connection facts");
            let bytes = receive(&mut socket, Duration::from_secs(15)).await?;
            let mut stop = state.stop.clone();
            let response = tokio::select! {
                result = dispatch(state, facts.remote_host, &bytes) => result?,
                _ = socket.recv() => anyhow::bail!("BLE channel closed or received an unexpected frame"),
                _ = stop.changed() => anyhow::bail!("BLE core stopped"),
            };
            socket.send(Message::Binary(response.into())).await?;
            Ok::<_, anyhow::Error>(())
        }).await;
    })
}
async fn receive(socket: &mut WebSocket, timeout: Duration) -> Result<Vec<u8>> {
    let frame = tokio::time::timeout(timeout, socket.recv())
        .await?
        .ok_or_else(|| anyhow::anyhow!("BLE channel closed"))??;
    let Message::Binary(bytes) = frame else {
        anyhow::bail!("Expected BLE bytes");
    };
    ble_wire::parts(&bytes)?;
    Ok(bytes.to_vec())
}
pub(super) async fn dispatch(
    state: ServerState,
    remote_host: String,
    data: &[u8],
) -> Result<Vec<u8>> {
    let request = Request::decode(data)?;
    let (mut builder, body) = match request {
        Request::PeerGraphql {
            client_id,
            channel_id,
            body,
        } => (
            axum::http::Request::builder()
                .method("POST")
                .uri("/peer_graphql")
                .header("c-id", client_id)
                .header("c-cid", channel_id),
            body,
        ),
        Request::FileChunk {
            client_id,
            file_id,
            offset,
            length,
        } => {
            let mut url = reqwest::Url::parse("http://localhost/fs")?;
            url.query_pairs_mut()
                .append_pair("id", &file_id)
                .append_pair("offset", &offset.to_string())
                .append_pair("length", &length.to_string());
            (
                axum::http::Request::builder()
                    .method("GET")
                    .uri(format!("/fs?{}", url.query().unwrap()))
                    .header("c-id", client_id)
                    .header("c-cid", ""),
                Vec::new(),
            )
        }
    };
    builder = builder.header("content-type", "application/octet-stream");
    let mut req = builder.body(Body::from(body))?;
    req.extensions_mut()
        .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
            [127, 0, 0, 1],
            0,
        ))));
    #[cfg(feature = "http_transport")]
    {
        req.extensions_mut()
            .insert(super::http_bridge::RemoteHost(remote_host));
        req.extensions_mut()
            .insert(crate::http_transport::ConnectionScheme("https"));
    }
    #[cfg(not(feature = "http_transport"))]
    let _ = remote_host;
    use tower::ServiceExt;
    let response = super::server::peer_router(state).oneshot(req).await?;
    let status = response.status().as_u16();
    let body = axum::body::to_bytes(response.into_body(), ble_wire::MAX_BODY).await?;
    ble_wire::response(status, &body)
}

#[cfg(test)]
impl Runtime {
    pub(super) fn request_for_test(&self, params: &serde_json::Value) -> Request {
        let pending = self.pending.lock().unwrap();
        Request::decode(&pending[params["token"].as_str().unwrap()].bytes).unwrap()
    }
    pub(super) fn reply_for_test(
        &self,
        params: &serde_json::Value,
        bytes: Vec<u8>,
    ) -> serde_json::Value {
        let entry = self
            .pending
            .lock()
            .unwrap()
            .remove(params["token"].as_str().unwrap())
            .unwrap();
        let _ = entry.reply.send(Ok(bytes));
        serde_json::json!(true)
    }
}
#[cfg(test)]
pub(super) fn test_peer_wire(runtime: &Runtime, params: &serde_json::Value, key: &[u8]) -> String {
    let Request::PeerGraphql { body, .. } = runtime.request_for_test(params) else {
        panic!("Expected peer request")
    };
    String::from_utf8(crate::xchacha_decrypt_raw(key, &body).unwrap()).unwrap()
}
#[cfg(test)]
pub(super) fn test_reply(
    runtime: &Runtime,
    params: &serde_json::Value,
    value: serde_json::Value,
    key: &[u8],
) -> serde_json::Value {
    let body = crate::xchacha_encrypt_raw(key, value.to_string().as_bytes()).unwrap();
    runtime.reply_for_test(params, ble_wire::response(200, &body).unwrap())
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/ble_http.rs"]
mod tests;
