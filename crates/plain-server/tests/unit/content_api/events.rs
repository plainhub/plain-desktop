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
        "MESSAGE_CREATED",
        "MESSAGE_DELETED",
        "MESSAGE_UPDATED",
        "NOTIFICATION_CREATED",
        "NOTIFICATION_UPDATED",
        "NOTIFICATION_DELETED",
        "NOTIFICATION_REFRESHED",
        "POMODORO_ACTION",
        "POMODORO_SETTINGS_UPDATE",
        "DEVICE_NAME_UPDATED",
        "DOWNLOAD_PROGRESS",
        "CHANNELS_UPDATED",
        "PEER_STATUS_UPDATED",
        "PAIRING_REQUEST_RECEIVED",
        "PAIRING_SUCCESS",
        "PAIRING_FAILED",
        "PAIRING_CANCELED",
        "PAIRING_STARTED",
        "NEARBY_DEVICE_FOUND",
        "NEARBY_DISCOVERY_STARTED",
        "NEARBY_DISCOVERY_STOPPED",
        "CONTENT_CHANGED",
        "MDNS_UPDATED",
        "NEARBY_DEVICES_UPDATED",
        "UNKNOWN_EVENT",
    ] {
        assert!(
            host_event(Message::Text(
                json!({"type":kind,"payload":"{}"}).to_string().into()
            ))
            .is_none(),
            "kind {kind}"
        );
        assert!(
            host_event(Message::Binary(
                crate::ws_frame::encode_raw(kind, &[]).unwrap().into()
            ))
            .is_none(),
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
        assert!(
            host_event(Message::Binary(
                crate::ws_frame::encode_raw(kind, &[]).unwrap().into()
            ))
            .is_none()
        );
    }
    for &kind in HOST_BINARY_TYPES {
        let mut body = crate::ws_frame::encode_raw(kind, &[]).unwrap();
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
    assert!(
        host_event(Message::Text(
            json!({"type":22,"payload":"{}"}).to_string().into()
        ))
        .is_none()
    );
    assert!(host_event(Message::Binary(31i32.to_le_bytes().to_vec().into())).is_none());
    assert!(!WsEvent::broadcast("PAIRING_REQUEST_RECEIVED", "{}".into()).is_host_event());
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
    assert_eq!(greeting["type"], "CONTENT_CHANGED");
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
            json!({"type":"SCREEN_MIRROR_VIDEO_CODEC","payload":"\"Synthetic\""})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let event = tokio::time::timeout(Duration::from_secs(2), public_events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event.event_type, "SCREEN_MIRROR_VIDEO_CODEC");
    assert_eq!(event.payload, "\"Synthetic\"");
    assert!(event.is_host_event());
    let mut binary = crate::ws_frame::encode_raw("SCREEN_MIRROR_VIDEO", &[]).unwrap();
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
        .send(WsEvent::broadcast("PAIRING_REQUEST_RECEIVED", "{}".into()))
        .unwrap();
    for socket in [&mut subscriber, &mut other] {
        let received = socket.next().await.unwrap().unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(received.to_text().unwrap()).unwrap()["type"],
            "PAIRING_REQUEST_RECEIVED"
        );
    }
    subscriber
        .send(ClientMessage::Text(
            json!({"type":"PAIRING_REQUEST_RECEIVED","payload":"{}"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    assert!(
        matches!(subscriber.next().await.unwrap().unwrap(), ClientMessage::Close(Some(frame)) if frame.code == tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Policy)
    );
    other
        .send(ClientMessage::Binary(
            crate::ws_frame::encode_raw("PAIRING_REQUEST_RECEIVED", &[])
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
    assert!(
        matches!(other.next().await.unwrap().unwrap(), ClientMessage::Close(Some(frame)) if frame.code == tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Policy)
    );
    assert_eq!(
        public_events.recv().await.unwrap().event_type,
        "PAIRING_REQUEST_RECEIVED"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(100), public_events.recv())
            .await
            .is_err()
    );
    server.shutdown().await;
}
