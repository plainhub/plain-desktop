use super::*;
fn fixture() -> (tempfile::TempDir, Prefs) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Prefs::load(&dir.path().join("system_prefs.json")).unwrap();
    (dir, prefs)
}
#[test]
fn defaults_are_complete_and_partial_patch_does_not_reset_other_settings() {
    let (_dir, prefs) = fixture();
    let settings = snapshot(&prefs).unwrap();
    assert_eq!(settings.http_port, 8080);
    assert!(settings.keep_awake);
    assert!(settings.desktop_access);
    assert_eq!(
        settings.home_features,
        "IMAGES|VIDEOS|AUDIO|DOCS|FILES|NOTES|FEEDS"
    );
    patch(
        &prefs,
        UserSettingsPatch {
            editor_font_size: Some(20),
            ..Default::default()
        },
    )
    .unwrap();
    patch(
        &prefs,
        UserSettingsPatch {
            keep_awake: Some(false),
            ..Default::default()
        },
    )
    .unwrap();
    let result = snapshot(&prefs).unwrap();
    assert_eq!(result.editor_font_size, 20);
    assert!(!result.keep_awake);
}
#[test]
fn recent_lists_deduplicate_and_enforce_their_limits() {
    let (_dir, prefs) = fixture();
    for i in 0..12 {
        Action::RecordSearch(format!("term{i}"))
            .apply(&prefs)
            .unwrap();
    }
    Action::RecordSearch("TERM5".into()).apply(&prefs).unwrap();
    let recent = snapshot(&prefs).unwrap().recent_searches;
    assert_eq!(recent.as_array().unwrap().len(), 10);
    assert_eq!(recent[0], "TERM5");
    assert_eq!(
        recent
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v.as_str().unwrap().eq_ignore_ascii_case("term5"))
            .count(),
        1
    );
    for i in 0..7 {
        Action::RecordSaveDirectory(format!("dir{i}"))
            .apply(&prefs)
            .unwrap();
    }
    Action::RecordSaveDirectory("dir3".into())
        .apply(&prefs)
        .unwrap();
    assert_eq!(
        snapshot(&prefs)
            .unwrap()
            .recent_save_dirs
            .as_array()
            .unwrap()
            .len(),
        5
    );
    Action::ClearSearches.apply(&prefs).unwrap();
    assert_eq!(
        snapshot(&prefs).unwrap().recent_searches,
        serde_json::json!([])
    );
}
#[test]
fn invalid_patch_and_failed_write_leave_store_unchanged() {
    let (dir, prefs) = fixture();
    let before = prefs.user_entries();
    assert!(
        patch(
            &prefs,
            UserSettingsPatch {
                http_port: Some(70000),
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(before, prefs.user_entries());
    std::fs::create_dir(dir.path().join("user_prefs.json.tmp")).unwrap();
    assert!(
        patch(
            &prefs,
            UserSettingsPatch {
                keep_awake: Some(false),
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(before, prefs.user_entries());
}
#[test]
fn notification_actions_are_atomic_and_reject_invalid_modes() {
    let (_dir, prefs) = fixture();
    std::thread::scope(|scope| {
        for i in 0..12 {
            let prefs = &prefs;
            scope.spawn(move || {
                Action::ToggleNotificationApp(format!("app{i}"))
                    .apply(prefs)
                    .unwrap();
            });
        }
    });
    assert_eq!(
        snapshot(&prefs).unwrap().notification_filter["apps"]
            .as_array()
            .unwrap()
            .len(),
        12
    );
    Action::SetNotificationMode("allowlist".into())
        .apply(&prefs)
        .unwrap();
    Action::ClearNotificationApps.apply(&prefs).unwrap();
    assert_eq!(
        snapshot(&prefs).unwrap().notification_filter,
        serde_json::json!({"mode":"allowlist","apps":[]})
    );
    assert!(
        Action::SetNotificationMode("other".into())
            .apply(&prefs)
            .is_err()
    );
}

#[test]
fn structured_values_reject_unusable_shapes_without_writing() {
    let (_dir, prefs) = fixture();
    let before = prefs.user_entries();
    for input in [
        UserSettingsPatch {
            notification_filter: Some(serde_json::json!({"mode":"other","apps":[]})),
            ..Default::default()
        },
        UserSettingsPatch {
            screen_mirror_quality: Some(serde_json::json!({"mode":"HD","resolution":0})),
            ..Default::default()
        },
        UserSettingsPatch {
            pomodoro_settings: Some(serde_json::json!({})),
            ..Default::default()
        },
        UserSettingsPatch {
            recent_searches: Some(serde_json::json!([1])),
            ..Default::default()
        },
        UserSettingsPatch {
            video_playlist: Some(serde_json::json!([{"path":"video"}])),
            ..Default::default()
        },
    ] {
        assert!(patch(&prefs, input).is_err());
        assert_eq!(before, prefs.user_entries());
    }
}

#[test]
fn recent_searches_keep_simple_unicode_case_matching() {
    let (_dir, prefs) = fixture();
    for (first, second) in [("İ", "i"), ("ς", "σ"), ("ß", "ẞ")] {
        Action::ClearSearches.apply(&prefs).unwrap();
        Action::RecordSearch(first.into()).apply(&prefs).unwrap();
        Action::RecordSearch(second.into()).apply(&prefs).unwrap();
        assert_eq!(
            snapshot(&prefs).unwrap().recent_searches,
            serde_json::json!([second])
        );
    }
    Action::ClearSearches.apply(&prefs).unwrap();
    Action::RecordSearch("ß".into()).apply(&prefs).unwrap();
    Action::RecordSearch("s".into()).apply(&prefs).unwrap();
    assert_eq!(
        snapshot(&prefs)
            .unwrap()
            .recent_searches
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
