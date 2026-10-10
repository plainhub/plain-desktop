use super::*;
#[tokio::test]
async fn replies_are_correlated_and_replacement_cannot_complete_new_calls() {
    let host = Arc::new(Host::default());
    let (generation, mut incoming) = host.connect();
    let call = {
        let host = host.clone();
        tokio::spawn(async move { host.call("first", json!({})).await })
    };
    let request = incoming.recv().await.unwrap();
    host.reply(generation, json!({"id":request["id"],"result":123}))
        .unwrap();
    assert_eq!(call.await.unwrap().unwrap(), json!(123));
    let call = {
        let host = host.clone();
        tokio::spawn(async move { host.call("second", json!({})).await })
    };
    let request = incoming.recv().await.unwrap();
    let (next, _) = host.connect();
    assert!(call.await.unwrap().unwrap_err().contains("replaced"));
    host.disconnect(generation);
    assert_eq!(host.state.lock().unwrap().generation, next);
    host.reply(generation, json!({"id":request["id"],"result":0}))
        .unwrap();
    assert!(host.state.lock().unwrap().sender.is_some());
    host.disconnect(next);
    assert!(host.state.lock().unwrap().sender.is_none());
}
#[tokio::test]
async fn cancel_and_disconnect_release_pending_requests() {
    let host = Arc::new(Host::default());
    let (generation, mut incoming) = host.connect();
    let call = {
        let host = host.clone();
        tokio::spawn(async move { host.call("cancel", json!({})).await })
    };
    incoming.recv().await.unwrap();
    call.abort();
    assert!(call.await.unwrap_err().is_cancelled());
    assert!(host.state.lock().unwrap().pending.is_empty());
    let call = {
        let host = host.clone();
        tokio::spawn(async move { host.call("disconnect", json!({})).await })
    };
    incoming.recv().await.unwrap();
    host.disconnect(generation);
    assert!(call.await.unwrap().unwrap_err().contains("disconnected"));
}
#[tokio::test]
async fn capacity_is_bounded_and_errors_are_not_empty_successes() {
    let host = Arc::new(Host::default());
    let (generation, mut incoming) = host.connect();
    let mut workers = vec![];
    for _ in 0..32 {
        let host = host.clone();
        workers.push(tokio::spawn(async move {
            host.call("pending", json!({})).await
        }));
        incoming.recv().await.unwrap();
    }
    assert!(
        host.call("overflow", json!({}))
            .await
            .unwrap_err()
            .contains("capacity")
    );
    host.disconnect(generation);
    for worker in workers {
        assert!(worker.await.unwrap().is_err());
    }
    assert!(host.state.lock().unwrap().pending.is_empty());
}
