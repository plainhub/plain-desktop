use super::*;

#[test]
fn settings_are_shared_and_decode_errors_are_not_defaults() {
    let directory = tempfile::tempdir().unwrap();
    let prefs = Prefs::load(&directory.path().join("prefs.json")).unwrap();
    assert_eq!(
        get_samba_settings(&prefs).unwrap(),
        SambaSettings::default()
    );
    let settings = SambaSettings {
        has_password: true,
        ..SambaSettings::default()
    };
    set_samba_settings(&prefs, &settings).unwrap();
    let configured = configure(
        &prefs,
        true,
        vec![SambaShare {
            name: "media".into(),
            share_path: "/mnt/media".into(),
            auth: SambaShareAuth::Password,
            read_only: true,
        }],
    )
    .unwrap();
    assert_eq!(get_samba_settings(&prefs).unwrap(), configured);
    assert!(configure(&prefs, true, vec![]).is_err());
    assert_eq!(get_samba_settings(&prefs).unwrap(), configured);
    prefs.set(KEY_SAMBA_SETTINGS, "invalid settings").unwrap();
    assert!(get_samba_settings(&prefs).is_err());
}

#[test]
fn password_shares_keep_the_existing_password_requirement() {
    let directory = tempfile::tempdir().unwrap();
    let prefs = Prefs::load(&directory.path().join("prefs.json")).unwrap();
    assert!(
        configure(
            &prefs,
            true,
            vec![SambaShare {
                name: "media".into(),
                share_path: "/mnt/media".into(),
                auth: SambaShareAuth::Password,
                read_only: false,
            }]
        )
        .is_err()
    );
    assert_eq!(
        get_samba_settings(&prefs).unwrap(),
        SambaSettings::default()
    );
}
