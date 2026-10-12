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
