#[tokio::test]
async fn pairing_identity_and_rename_use_the_same_core_preferences() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let pairing = core.native_pairing().unwrap();
    let identity = pairing.identity().unwrap();
    assert_eq!(
        identity.client_id,
        core.native_graphql().unwrap().client_id()
    );
    assert_eq!(
        plain_rs::utils::base64::base64_decode(&identity.public_key).len(),
        32
    );
    assert!(
        pairing
            .set_device_name("Shared desktop".into())
            .await
            .unwrap()
    );
    assert_eq!(pairing.identity().unwrap().device_name, "Shared desktop");
    assert_eq!(pairing.identity().unwrap().client_id, identity.client_id);
    core.stop_core().await;
    assert!(core.native_pairing().is_err());
    assert!(pairing.identity().is_err());
    assert!(pairing.cancel("missing").await.is_err());
    assert!(
        pairing
            .set_device_name("Stopped desktop".into())
            .await
            .is_err()
    );
}
