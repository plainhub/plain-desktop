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
    kind: i32,
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
            json!({"type":47,"payload":"{}","hostCapabilities":{"textTypes":HOST_TEXT_TYPES,"binaryTypes":HOST_BINARY_TYPES}}).to_string().into(),
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
                                let mut frame = event.event_type.to_le_bytes().to_vec();
                                frame.extend(payload);
                                Message::Binary(frame.into())
                            },
                            None => Message::Text(json!({"type":event.event_type,"payload":event.payload}).to_string().into()),
                        }
                    },
                    Err(broadcast::error::RecvError::Lagged(_)) => Message::Text(json!({"type":47,"payload":"{}"}).to_string().into()),
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
            WsEvent::host_text(packet.kind, packet.payload)
        }
        Message::Binary(bytes) if bytes.len() >= 4 => {
            let kind = i32::from_le_bytes(bytes[..4].try_into().ok()?);
            WsEvent::host_binary(kind, bytes[4..].to_vec())
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/events.rs"]
mod tests;
