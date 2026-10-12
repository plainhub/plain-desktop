use super::*;
use tokio::sync::{broadcast, watch};

const CID: &str = "window-main";
const KEY: [u8; 32] = [7u8; 32];

fn text_event(payload: &str) -> WsEvent {
    WsEvent::broadcast("CHAT_MESSAGE", payload.to_string())
}

async fn wait_for_stop(mut stopped: watch::Receiver<bool>) {
    let _ = stopped.wait_for(|stopped| *stopped).await;
}

#[tokio::test]
async fn delivers_a_broadcast_event() {
    let (sender, mut events) = broadcast::channel(8);
    let (_stop, stopped) = watch::channel(false);
    sender.send(text_event("first")).expect("send");

    let event = next_event(&mut events, || wait_for_stop(stopped.clone()))
        .await
        .expect("an event before any stop");

    assert_eq!(event.payload, "first");
}

#[tokio::test]
async fn stops_waiting_once_the_core_stops() {
    let (_sender, mut events) = broadcast::channel::<WsEvent>(8);
    let (stop, stopped) = watch::channel(false);
    stop.send(true).expect("stop the core");

    let event = next_event(&mut events, || wait_for_stop(stopped.clone())).await;

    assert!(event.is_none(), "a stopped core ends the subscription");
}

#[tokio::test]
async fn ends_when_the_event_channel_closes() {
    let (sender, mut events) = broadcast::channel::<WsEvent>(8);
    let (_stop, stopped) = watch::channel(false);
    drop(sender);

    let event = next_event(&mut events, || wait_for_stop(stopped.clone())).await;

    assert!(event.is_none(), "a closed channel ends the subscription");
}

#[tokio::test]
async fn a_stopped_core_wins_over_a_queued_event() {
    let (sender, mut events) = broadcast::channel(8);
    let (stop, stopped) = watch::channel(false);
    sender.send(text_event("queued")).expect("send");
    stop.send(true).expect("stop the core");

    let event = next_event(&mut events, || wait_for_stop(stopped.clone())).await;

    assert!(
        event.is_none(),
        "a stopped core drops the remaining frames instead of holding the window open"
    );
}

#[tokio::test]
async fn lagging_keeps_the_subscription_alive() {
    let (sender, mut events) = broadcast::channel(2);
    let (_stop, stopped) = watch::channel(false);
    for index in 0..5 {
        sender.send(text_event(&index.to_string())).expect("send");
    }

    let event = next_event(&mut events, || wait_for_stop(stopped.clone()))
        .await
        .expect("a retained event after the gap");

    assert!(
        event.payload == "3" || event.payload == "4",
        "a lagged receiver resumes on a retained event, got {}",
        event.payload
    );
}

#[test]
fn internal_events_are_not_delivered() {
    let event = WsEvent::broadcast("MDNS_UPDATED", "{}".to_string());

    assert!(encode_event(&event, CID, &KEY).is_none());
}

#[test]
fn events_addressed_to_another_client_are_not_delivered() {
    let event = WsEvent::targeted("CHAT_MESSAGE", "{}".to_string(), "window-chat");

    assert!(encode_event(&event, CID, &KEY).is_none());
}

#[test]
fn events_addressed_to_this_client_are_delivered() {
    let event = WsEvent::targeted("CHAT_MESSAGE", "{}".to_string(), CID);

    assert_eq!(
        ws_frame::decode(
            &encode_event(&event, CID, &KEY).expect("an encodable frame"),
            &KEY
        ),
        Some(("CHAT_MESSAGE".to_string(), b"{}".to_vec()))
    );
}

#[test]
fn broadcast_events_reach_every_client() {
    let event = text_event("broadcast");

    assert_eq!(
        ws_frame::decode(
            &encode_event(&event, "window-chat", &KEY).expect("an encodable frame"),
            &KEY
        ),
        Some(("CHAT_MESSAGE".to_string(), b"broadcast".to_vec()))
    );
}

#[test]
fn binary_events_keep_the_raw_frame_bytes() {
    let event = WsEvent::broadcast_binary("SCREEN_MIRROR_VIDEO", vec![0, 159, 146, 150]);

    assert_eq!(
        encode_event(&event, CID, &KEY),
        ws_frame::encode_raw("SCREEN_MIRROR_VIDEO", &[0, 159, 146, 150])
    );
}

#[test]
fn text_events_are_encrypted_with_the_url_token() {
    let event = text_event("{\"id\":1}");
    let frame = encode_event(&event, CID, &KEY).expect("an encodable frame");

    assert_eq!(
        ws_frame::decode(&frame, &KEY),
        Some(("CHAT_MESSAGE".to_string(), b"{\"id\":1}".to_vec()))
    );
    assert_ne!(
        frame,
        ws_frame::encode_raw("CHAT_MESSAGE", b"{\"id\":1}").expect("a raw frame"),
        "a text frame must not stay readable on the wire"
    );
    assert!(
        !frame
            .windows(9)
            .any(|window| window == b"CHAT_MESS".get(..6).unwrap_or(&[])),
        "the event name must be the only plaintext part of the frame"
    );
}
