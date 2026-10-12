#[tokio::test]
async fn login_logout_and_rename_keep_pairing_in_the_core() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let peers = core.native_peers().unwrap();
    peers
        .login(super::RemoteLogin {
            id: "peer-a".into(),
            name: "Peer A".into(),
            host: "127.0.0.1:9443".into(),
            device_type: plain_rs::chat::enums::DeviceType::Computer,
            token: "session".into(),
            signature_public_key: "public".into(),
            chat_key: "pairing-key".into(),
        })
        .unwrap();
    assert_eq!(peers.logged_in().unwrap()[0].port, 9443);
    peers.update_name("peer-a", "Renamed peer").unwrap();
    assert_eq!(peers.logged_in().unwrap()[0].name, "Renamed peer");
    peers.logout("peer-a").unwrap();
    assert!(peers.logged_in().unwrap().is_empty());
    let state = core.runtime_state();
    let row = plain_rs::db::chat_store::peers::get(&state.db, "peer-a")
        .unwrap()
        .unwrap();
    assert_eq!(row.status, plain_rs::chat::enums::PeerStatus::Paired);
    assert_eq!(row.key, "pairing-key");
    core.stop_core().await;
    assert!(core.native_peers().is_err());
    assert!(peers.logged_in().is_err());
    assert!(peers.logout("peer-a").is_err());
}
