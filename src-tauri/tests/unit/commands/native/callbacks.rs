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
            let InvokeResponseBody::Json(json) = body else { panic!("JSON callback required") };
            sink.lock().unwrap().push(serde_json::from_str(&json).unwrap());
            Ok(())
        });
        target.lock().unwrap().insert("view".into(), (1, channel));
        callbacks.deliver(method, &serde_json::json!({"method":method})).unwrap();
    }
    assert_eq!(values.lock().unwrap().len(), 2);
    callbacks.remove_view("view");
    callbacks.deliver("networkInterfacesChanged", &Value::Null).unwrap();
    callbacks.deliver("peerClipboardChanged", &Value::Null).unwrap();
    assert_eq!(values.lock().unwrap().len(), 2);
}
