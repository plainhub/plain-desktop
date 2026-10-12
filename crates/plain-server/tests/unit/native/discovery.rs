#[tokio::test]
async fn hostname_uses_shared_settings_and_stopped_discovery_rejects_operations() {
    let directory = tempfile::tempdir().unwrap();
    let core = crate::assembly::server::ContentServer::open(directory.path()).unwrap();
    let discovery = core.native_discovery().unwrap();
    discovery.set_hostname(" renderer.local ").await.unwrap();
    assert_eq!(discovery.hostname().unwrap(), "renderer.local");
    assert_eq!(
        plain_rs::prefs::system::snapshot(&core.runtime_state().prefs)
            .unwrap()
            .mdns_hostname,
        "renderer.local"
    );
    assert!(discovery.set_hostname("invalid.example").await.is_err());
    assert_eq!(discovery.hostname().unwrap(), "renderer.local");
    assert!(!discovery.stop_browse().await.unwrap());
    assert!(
        !core.runtime_state().mdns.snapshot()["receiver"]
            .as_bool()
            .unwrap()
    );
    core.stop_core().await;
    assert!(core.native_discovery().is_err());
    assert!(discovery.hostname().is_err());
    assert!(discovery.set_hostname("other.local").await.is_err());
    assert!(discovery.browse().await.is_err());
    assert!(discovery.start_browse().await.is_err());
    assert!(discovery.stop_browse().await.is_err());
}
