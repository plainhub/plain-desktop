use super::*;
use plain_rs::features::{
    access::AccessFeature,
    prefs::{
        Prefs,
        pref_keys::{API_PERMISSIONS, LOGIN_REQUIRE_APPROVAL},
        security::{PasswordType, password_type},
    },
};
use plain_rs::infrastructure::data_paths::DataPaths;
use std::path::PathBuf;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let token = plain_libs::crypto::gen_token().replace(['/', '+'], "_");
        let root = std::env::temp_dir().join(format!("plain-desktop-defaults-{token}"));
        std::fs::create_dir(&root).unwrap();
        Self(root.canonicalize().unwrap())
    }

    fn data(&self) -> PathBuf {
        self.0.join("data")
    }

    fn open(&self) -> ContentServer {
        open_desktop_core(&self.data(), &self.0.join("cache")).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn permissions(core: &ContentServer) -> Option<Vec<AccessFeature>> {
    core.shared_preferences()
        .unwrap()
        .get(API_PERMISSIONS)
        .unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn fresh_desktop_defaults_once_and_preserves_explicit_off_on_restart() {
    let fixture = Fixture::new();
    let first = fixture.open();
    assert_eq!(
        permissions(&first),
        Some(AccessFeature::FILE_FEATURES.to_vec())
    );
    let prefs = first.shared_preferences().unwrap();
    assert_eq!(prefs.get(LOGIN_REQUIRE_APPROVAL).unwrap(), Some(true));
    assert_eq!(password_type(&prefs).unwrap(), PasswordType::None);
    prefs
        .set(API_PERMISSIONS, Vec::<AccessFeature>::new())
        .unwrap();
    first.shutdown().await;
    let second = fixture.open();
    assert_eq!(permissions(&second), Some(vec![]));
    second.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn existing_data_without_permissions_is_not_enabled() {
    let fixture = Fixture::new();
    fixture.open().shutdown().await;
    let prefs_path = DataPaths::new(&fixture.data()).prefs();
    if prefs_path.exists() {
        std::fs::remove_file(prefs_path).unwrap();
    }
    let core = fixture.open();
    assert_eq!(permissions(&core), None);
    core.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn existing_preferences_are_never_overwritten() {
    for enabled in [vec![], vec![AccessFeature::Images]] {
        let fixture = Fixture::new();
        std::fs::create_dir(fixture.data()).unwrap();
        let prefs = Prefs::load(&DataPaths::new(&fixture.data()).prefs()).unwrap();
        prefs.set(API_PERMISSIONS, enabled.clone()).unwrap();
        let core = fixture.open();
        assert_eq!(permissions(&core), Some(enabled));
        core.shutdown().await;
    }
}
