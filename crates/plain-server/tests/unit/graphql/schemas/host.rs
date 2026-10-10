use super::*;
#[test]
fn host_settings_schema_matches_committed_sdl() {
    let dir = tempfile::tempdir().unwrap();
    let prefs = std::sync::Arc::new(
        plain_rs::prefs::Prefs::load(&dir.path().join("system_prefs.json")).unwrap(),
    );
    let db = std::sync::Arc::new(plain_rs::db::Db::open(std::path::Path::new(":memory:")).unwrap());
    let (events, _) = tokio::sync::broadcast::channel(16);
    let sdl = build(db, events, prefs, dir.path().into()).sdl();
    assert!(sdl.contains("systemSettings: SystemSettings!"));
    assert!(sdl.contains("userSettings: UserSettings!"));
    assert!(sdl.contains("systemPrefs: JSON!"));
    if std::env::var_os("UPDATE_CONTENT_SCHEMA").is_some() {
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../plain-rs/testdata/content-api.graphqls"
            ),
            &sdl,
        )
        .unwrap();
    } else {
        assert_eq!(
            sdl,
            include_str!("../../../../../../plain-rs/testdata/content-api.graphqls")
        );
    }
}
