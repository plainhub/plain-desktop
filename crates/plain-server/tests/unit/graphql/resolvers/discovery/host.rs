#[tokio::test]
async fn nearby_snapshot_is_native_private_and_does_not_start_discovery() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let native = core.native_graphql().unwrap();
    assert!(
        core.runtime_state()
            .public
            .sdl()
            .contains("startDiscovery: Boolean!")
    );
    assert!(
        core.runtime_state()
            .schema
            .sdl()
            .contains("startDiscovery: [NearbyDevice!]!")
    );
    let stopped = native
        .execute_private(async_graphql::Request::new("mutation { stopDiscovery }"))
        .await;
    assert!(stopped.errors.is_empty(), "{:?}", stopped.errors);
    assert_eq!(
        stopped.data.into_json().unwrap()["stopDiscovery"],
        serde_json::json!(false)
    );

    let selection = "{ nearbyDevices { id name ips port deviceType version platform lastSeen discoveryMethods bestIp status } }";
    let response = native
        .execute_private(async_graphql::Request::new(selection))
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        response.data.into_json().unwrap()["nearbyDevices"],
        serde_json::json!([])
    );
    assert!(
        !core.runtime_state().mdns.snapshot()["scanning"]
            .as_bool()
            .unwrap()
    );
    let response = native.execute(async_graphql::Request::new(selection)).await;
    assert!(!response.errors.is_empty());
    core.stop_core().await;
    let response = native
        .execute_private(async_graphql::Request::new(selection))
        .await;
    assert!(!response.errors.is_empty());
}
