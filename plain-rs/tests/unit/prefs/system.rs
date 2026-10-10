use super::*;
fn fixture() -> (tempfile::TempDir, Prefs) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Prefs::load(&dir.path().join("system_prefs.json")).unwrap();
    (dir, prefs)
}
#[test]
fn bootstrap_is_stable_and_snapshot_never_contains_private_secrets() {
    let (_dir, prefs) = fixture();
    bootstrap(&prefs).unwrap();
    let first = serde_json::to_value(snapshot(&prefs).unwrap()).unwrap();
    bootstrap(&prefs).unwrap();
    assert_eq!(
        first,
        serde_json::to_value(snapshot(&prefs).unwrap()).unwrap()
    );
    assert_eq!(first["passwordType"], 2);
    assert_eq!(first["authTwoFactor"], true);
    assert!(!first["clientId"].as_str().unwrap().is_empty());
    assert_eq!(signing_keypair(&prefs).unwrap().len(), 64);
    assert!(first.get("masterSecret").is_none());
    assert!(first.get("signatureKeyPair").is_none());
}
#[test]
fn url_rotation_is_a_bootstrap_policy_and_explicit_reset_preserves_identity() {
    let (_dir, prefs) = fixture();
    bootstrap(&prefs).unwrap();
    let identity = prefs.get::<String>(CLIENT_ID).unwrap();
    let first = prefs.get::<String>(URL_TOKEN).unwrap();
    Command::SetRotateUrlToken { enabled: true }
        .apply(&prefs)
        .unwrap();
    assert_eq!(first, prefs.get::<String>(URL_TOKEN).unwrap());
    bootstrap(&prefs).unwrap();
    assert_ne!(first, prefs.get::<String>(URL_TOKEN).unwrap());
    Command::ResetUrlToken.apply(&prefs).unwrap();
    assert_eq!(identity, prefs.get::<String>(CLIENT_ID).unwrap());
}
#[test]
fn parallel_permission_updates_do_not_lose_changes() {
    let (_dir, prefs) = fixture();
    bootstrap(&prefs).unwrap();
    std::thread::scope(|scope| {
        for name in ["CAMERA", "ADB", "READ_SMS", "CLIPBOARD"] {
            let prefs = &prefs;
            scope.spawn(move || {
                Command::SetApiPermission {
                    permission: name.into(),
                    enabled: true,
                }
                .apply(prefs)
                .unwrap();
            });
        }
    });
    assert_eq!(snapshot(&prefs).unwrap().api_permissions.len(), 4);
    assert!(
        Command::SetApiPermission {
            permission: "UNKNOWN".into(),
            enabled: true
        }
        .apply(&prefs)
        .is_err()
    );
}
#[test]
fn update_patch_preserves_other_fields_and_write_failure_does_not_publish_state() {
    let (dir, prefs) = fixture();
    bootstrap(&prefs).unwrap();
    Command::PatchUpdate {
        patch: serde_json::from_value(
            serde_json::json!({"skipVersion":"4.0","autoCheckUpdate":false}),
        )
        .unwrap(),
    }
    .apply(&prefs)
    .unwrap();
    Command::PatchUpdate {
        patch: serde_json::from_value(serde_json::json!({"downloadedApkPath":"download.apk"}))
            .unwrap(),
    }
    .apply(&prefs)
    .unwrap();
    let info = snapshot(&prefs).unwrap().update_info;
    assert_eq!(info.skip_version, "4.0");
    assert!(!info.auto_check_update);
    let before = prefs.entries();
    std::fs::create_dir(dir.path().join("system_prefs.json.tmp")).unwrap();
    assert!(Command::ResetPassword.apply(&prefs).is_err());
    assert_eq!(before, prefs.entries());
}
