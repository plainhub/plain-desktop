use super::*;
use std::sync::Arc;
use tauri::ipc::InvokeResponseBody;

#[test]
fn callbacks_deliver_only_to_registered_channels_and_remove_closed_views() {
    let callbacks = HostCallbacks::default();
    let values = Arc::new(Mutex::new(Vec::<Value>::new()));
    let sink = values.clone();
    let channel = Channel::new(move |body| {
        let InvokeResponseBody::Json(json) = body else {
            panic!("JSON callback required")
        };
        sink.lock()
            .unwrap()
            .push(serde_json::from_str(&json).unwrap());
        Ok(())
    });
    callbacks
        .notifications
        .lock()
        .unwrap()
        .insert("view".into(), (1, channel));
    callbacks
        .deliver(
            "peerNotificationsChanged",
            &serde_json::json!({"revision":1}),
        )
        .unwrap();
    assert_eq!(
        values.lock().unwrap().as_slice(),
        &[serde_json::json!({"revision":1})]
    );
    assert!(callbacks.deliver("unknown", &Value::Null).is_err());
    callbacks.remove_view("view");
    callbacks
        .deliver(
            "peerNotificationsChanged",
            &serde_json::json!({"revision":2}),
        )
        .unwrap();
    assert_eq!(values.lock().unwrap().len(), 1);
}

#[test]
fn network_and_clipboard_callbacks_are_scoped_to_the_registered_view() {
    let callbacks = HostCallbacks::default();
    let values = Arc::new(Mutex::new(Vec::<Value>::new()));
    for (method, target) in [
        ("networkInterfacesChanged", &callbacks.network),
        ("peerClipboardChanged", &callbacks.clipboard),
    ] {
        let sink = values.clone();
        let channel = Channel::new(move |body| {
            let InvokeResponseBody::Json(json) = body else {
                panic!("JSON callback required")
            };
            sink.lock()
                .unwrap()
                .push(serde_json::from_str(&json).unwrap());
            Ok(())
        });
        target.lock().unwrap().insert("view".into(), (1, channel));
        callbacks
            .deliver(method, &serde_json::json!({"method":method}))
            .unwrap();
    }
    assert_eq!(values.lock().unwrap().len(), 2);
    callbacks.remove_view("view");
    callbacks
        .deliver("networkInterfacesChanged", &Value::Null)
        .unwrap();
    callbacks
        .deliver("peerClipboardChanged", &Value::Null)
        .unwrap();
    assert_eq!(values.lock().unwrap().len(), 2);
}

#[test]
fn online_clients_replay_the_latest_snapshot_and_stop_delivery_to_closed_views() {
    let callbacks = HostCallbacks::default();
    callbacks
        .deliver(
            "onlineClientsChanged",
            &serde_json::json!({"clientIds":["client-a"]}),
        )
        .unwrap();
    let values = Arc::new(Mutex::new(Vec::<Value>::new()));
    let sink = values.clone();
    let channel = Channel::new(move |body| {
        let InvokeResponseBody::Json(json) = body else {
            panic!("JSON callback required")
        };
        sink.lock()
            .unwrap()
            .push(serde_json::from_str(&json).unwrap());
        Ok(())
    });
    callbacks
        .online_clients
        .lock()
        .unwrap()
        .register("main".into(), 1, channel)
        .unwrap();
    assert_eq!(
        values.lock().unwrap().as_slice(),
        &[serde_json::json!({"clientIds":["client-a"]})]
    );
    callbacks
        .deliver("onlineClientsChanged", &serde_json::json!({"clientIds":[]}))
        .unwrap();
    assert_eq!(
        values.lock().unwrap().last().unwrap(),
        &serde_json::json!({"clientIds":[]})
    );
    callbacks.remove_view("main");
    callbacks
        .deliver(
            "onlineClientsChanged",
            &serde_json::json!({"clientIds":["client-b"]}),
        )
        .unwrap();
    assert_eq!(values.lock().unwrap().len(), 2);
    let cached = callbacks
        .online_clients
        .lock()
        .unwrap()
        .snapshot
        .clone()
        .unwrap();
    assert_eq!(cached.client_ids, vec!["client-b"]);
    assert!(
        callbacks
            .deliver(
                "onlineClientsChanged",
                &serde_json::json!({"unexpected":true})
            )
            .is_err()
    );
    assert_eq!(
        callbacks
            .online_clients
            .lock()
            .unwrap()
            .snapshot
            .as_ref()
            .unwrap()
            .client_ids,
        vec!["client-b"]
    );
}
