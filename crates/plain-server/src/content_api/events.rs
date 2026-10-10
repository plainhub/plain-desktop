use super::server::ServerState;
use crate::ws_event::{HOST_BINARY_TYPES, HOST_TEXT_TYPES, WsEvent};
use axum::{
    extract::{
        State, WebSocketUpgrade,
        ws::{CloseFrame, Message, WebSocket},
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::broadcast;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HostEventPacket {
    #[serde(rename = "type")]
    kind: String,
    payload: String,
}

pub(super) async fn subscribe(
    State(state): State<ServerState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let receiver = state.events.subscribe();
    ws.on_upgrade(move |socket| receive(socket, state, receiver))
        .into_response()
}

fn close(reason: &'static str) -> Message {
    Message::Close(Some(CloseFrame {
        code: 1008,
        reason: reason.into(),
    }))
}

async fn receive(
    mut socket: WebSocket,
    mut state: ServerState,
    mut receiver: broadcast::Receiver<WsEvent>,
) {
    if socket
        .send(Message::Text(
            json!({"type":"CONTENT_CHANGED","payload":"{}","hostCapabilities":{"textTypes":HOST_TEXT_TYPES,"binaryTypes":HOST_BINARY_TYPES}}).to_string().into(),
        ))
        .await
        .is_err()
    {
        return;
    }
    loop {
        tokio::select! {
            _ = state.stop.changed() => break,
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Ping(data))) => if socket.send(Message::Pong(data)).await.is_err() { break; },
                Some(Ok(Message::Pong(_))) => {},
                Some(Ok(message @ (Message::Text(_) | Message::Binary(_)))) => {
                    let Some(event) = host_event(message) else { let _ = socket.send(close("host_event_type_not_allowed")).await; break; };
                    let _ = state.events.send(event);
                },
                _ => break,
            },
            event = receiver.recv() => {
                let message = match event {
                    Ok(event) => {
                        if event.is_host_event() { continue; }
                        match event.binary_payload {
                            Some(payload) => {
                                let Some(frame) = crate::ws_frame::encode_raw(event.event_type, &payload) else { continue; };
                                Message::Binary(frame.into())
                            },
                            None => Message::Text(json!({"type":event.event_type,"payload":event.payload}).to_string().into()),
                        }
                    },
                    Err(broadcast::error::RecvError::Lagged(_)) => Message::Text(json!({"type":"CONTENT_CHANGED","payload":"{}"}).to_string().into()),
                    Err(_) => break,
                };
                if socket.send(message).await.is_err() { break; }
            }
        }
    }
}

fn host_event(message: Message) -> Option<WsEvent> {
    match message {
        Message::Text(text) => {
            let packet: HostEventPacket = serde_json::from_str(&text).ok()?;
            WsEvent::host_text(&packet.kind, packet.payload)
        }
        Message::Binary(bytes) => {
            let (kind, payload) = crate::ws_frame::decode_raw(&bytes)?;
            WsEvent::host_binary(kind, payload.to_vec())
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/events.rs"]
mod tests;
