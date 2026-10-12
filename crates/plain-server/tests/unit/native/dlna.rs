#[tokio::test]
async fn native_receiver_uses_core_state_and_refuses_stopped_operations() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let receiver = core.native_dlna().unwrap();
    assert_eq!(receiver.snapshot().await.unwrap()["enabled"], false);
    assert!(receiver.set_enabled(true).await.is_err());
    assert_eq!(receiver.snapshot().await.unwrap()["enabled"], true);
    assert_eq!(receiver.snapshot().await.unwrap()["isRunning"], false);
    receiver.set_enabled(false).await.unwrap();
    assert_eq!(receiver.snapshot().await.unwrap()["enabled"], false);
    assert!(receiver.senders().unwrap().allowed.is_empty());
    core.stop_core().await;
    assert!(core.native_dlna().is_err());
    assert!(receiver.snapshot().await.is_err());
    assert!(receiver.set_enabled(false).await.is_err());
    assert!(receiver.accept(false).await.is_err());
    assert!(receiver.reject(false).await.is_err());
    assert!(receiver.senders().is_err());
    assert!(
        receiver
            .remove_sender(true, "127.0.0.1".into())
            .await
            .is_err()
    );
}

/// The desktop bootstrap starts the receiver through this call once the
/// public listener is up, so the receiver has to come up on the port that
/// listener actually bound — not on a configured or preferred one.
#[tokio::test]
async fn enabling_the_receiver_starts_it_on_the_public_port() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let (http, _) = core
        .start_public(
            0,
            0,
            cert.cert.pem().into_bytes(),
            cert.key_pair.serialize_pem().into_bytes(),
        )
        .await
        .unwrap();
    assert_ne!(http, 0, "the listener has to own a real port");
    // The receiver takes a multicast lease before it advertises, which is
    // the one host primitive the desktop shell supplies here.
    let state = core.runtime_state();
    let host = state.host.clone();
    let (generation, mut requests) = host.connect();
    tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            assert_eq!(request["method"], "mdnsMulticast");
            let acquire = request["params"]["acquire"].as_bool().unwrap();
            let _ = host.reply(
                generation,
                serde_json::json!({"id":request["id"],"result":acquire}),
            );
        }
    });
    let receiver = core.native_dlna().unwrap();
    receiver.set_enabled(true).await.unwrap();
    let snapshot = receiver.snapshot().await.unwrap();
    assert_eq!(snapshot["enabled"], true);
    assert_eq!(
        snapshot["isRunning"], true,
        "an enabled receiver with a live listener has to be running"
    );
    assert_eq!(
        snapshot["port"], http,
        "the receiver advertises the port the listener bound"
    );
    receiver.set_enabled(false).await.unwrap();
    assert_eq!(receiver.snapshot().await.unwrap()["isRunning"], false);
    core.shutdown().await;
}
