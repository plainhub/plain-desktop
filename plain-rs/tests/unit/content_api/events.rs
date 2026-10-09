use super::super::ContentServer;
use super::*;
use futures_util::{SinkExt, StreamExt};
use std::{sync::Arc, time::Duration};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message as ClientMessage, client::IntoClientRequest},
};

fn request(port: u16, token: &str) -> tokio_tungstenite::tungstenite::http::Request<()> {
    let mut request = format!("ws://127.0.0.1:{port}/events")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    request
}

#[test]
fn host_uploads_cannot_forge_business_or_internal_state_events() {
    for kind in [
        -1i32, 1, 2, 3, 7, 8, 9, 10, 11, 12, 21, 16, 18, 20, 22, 23, 24, 25, 26, 27, 29, 30, 47,
        10001, 10011,
    ] {
        assert!(
            host_event(Message::Text(
                json!({"type":kind,"payload":"{}"}).to_string().into()
            ))
            .is_none(),
            "kind {kind}"
        );
        assert!(
            host_event(Message::Binary(kind.to_le_bytes().to_vec().into())).is_none(),
            "binary kind {kind}"
        );
    }
    for &kind in HOST_TEXT_TYPES {
        let event = host_event(Message::Text(
            json!({"type":kind,"payload":"{}"}).to_string().into(),
        ))
        .unwrap();
        assert_eq!(event.event_type, kind);
        assert!(event.is_host_event());
        assert!(host_event(Message::Binary(kind.to_le_bytes().to_vec().into())).is_none());
    }
    for &kind in HOST_BINARY_TYPES {
        let mut body = kind.to_le_bytes().to_vec();
        body.extend([1, 2, 3]);
        let event = host_event(Message::Binary(body.into())).unwrap();
        assert_eq!(event.binary_payload, Some(vec![1, 2, 3]));
        assert!(event.is_host_event());
        assert!(
            host_event(Message::Text(
                json!({"type":kind,"payload":"{}"}).to_string().into()
            ))
            .is_none()
        );
    }
    assert!(!WsEvent::broadcast(22, "{}".into()).is_host_event());
}

#[tokio::test]
async fn shared_event_socket_routes_platform_facts_outward_and_rejects_state_replay() {
    let dir = tempfile::tempdir().unwrap();
    let token = crate::base64_encode(&[9; 32]);
    let server = ContentServer::start(
        &dir.path().join("db"),
        &token,
        Arc::new(crate::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap()),
    )
    .unwrap();
    assert!(
        connect_async(request(server.port, "invalid"))
            .await
            .is_err()
    );
    let (mut subscriber, _) = connect_async(request(server.port, &token)).await.unwrap();
    let initial = subscriber.next().await.unwrap().unwrap();
    let greeting: serde_json::Value = serde_json::from_str(initial.to_text().unwrap()).unwrap();
    assert_eq!(greeting["type"], 47);
    assert_eq!(
        greeting["hostCapabilities"]["textTypes"],
        json!(HOST_TEXT_TYPES)
    );
    assert_eq!(
        greeting["hostCapabilities"]["binaryTypes"],
        json!(HOST_BINARY_TYPES)
    );
    let (mut other, _) = connect_async(request(server.port, &token)).await.unwrap();
    other.next().await.unwrap().unwrap();
    let mut public_events = server.runtime_state().events.subscribe();
    subscriber
        .send(ClientMessage::Text(
            json!({"type":32,"payload":"\"Synthetic\""})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let event = tokio::time::timeout(Duration::from_secs(2), public_events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event.event_type, 32);
    assert_eq!(event.payload, "\"Synthetic\"");
    assert!(event.is_host_event());
    let mut binary = 31i32.to_le_bytes().to_vec();
    binary.extend([1, 2, 3]);
    subscriber
        .send(ClientMessage::Binary(binary.into()))
        .await
        .unwrap();
    let event = tokio::time::timeout(Duration::from_secs(2), public_events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event.binary_payload, Some(vec![1, 2, 3]));
    assert!(event.is_host_event());
    assert!(
        tokio::time::timeout(Duration::from_millis(100), subscriber.next())
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(100), other.next())
            .await
            .is_err()
    );
    server
        .runtime_state()
        .events
        .send(WsEvent::broadcast(22, "{}".into()))
        .unwrap();
    for socket in [&mut subscriber, &mut other] {
        let received = socket.next().await.unwrap().unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(received.to_text().unwrap()).unwrap()["type"],
            22
        );
    }
    subscriber
        .send(ClientMessage::Text(
            json!({"type":22,"payload":"{}"}).to_string().into(),
        ))
        .await
        .unwrap();
    assert!(
        matches!(subscriber.next().await.unwrap().unwrap(), ClientMessage::Close(Some(frame)) if frame.code == tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Policy)
    );
    other
        .send(ClientMessage::Binary(22i32.to_le_bytes().to_vec().into()))
        .await
        .unwrap();
    assert!(
        matches!(other.next().await.unwrap().unwrap(), ClientMessage::Close(Some(frame)) if frame.code == tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Policy)
    );
    assert_eq!(public_events.recv().await.unwrap().event_type, 22);
    assert!(
        tokio::time::timeout(Duration::from_millis(100), public_events.recv())
            .await
            .is_err()
    );
    server.shutdown().await;
}
