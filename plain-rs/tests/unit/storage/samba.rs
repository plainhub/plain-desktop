//! Unit tests for `src/samba.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

fn tmp_prefs() -> crate::prefs::Prefs {
    let dir = tempfile::tempdir().unwrap();
    let prefs = crate::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap();
    std::mem::forget(dir);
    prefs
}

fn settings(enabled: bool, shares: Vec<SambaShare>) -> SambaSettings {
    SambaSettings {
        enabled,
        shares,
        ..Default::default()
    }
}

fn share(name: &str, path: &str, auth: SambaShareAuth, read_only: bool) -> SambaShare {
    SambaShare {
        name: name.into(),
        share_path: path.into(),
        auth,
        read_only,
    }
}

const NO_VFS: VfsCaps = VfsCaps {
    fruit: false,
    catia: false,
    streams_xattr: false,
};

#[test]
fn default_when_no_record() {
    let prefs = tmp_prefs();
    let s = get_samba_settings(&prefs);
    assert!(!s.enabled);
    assert!(s.shares.is_empty());
    assert_eq!(s.username, "nas");
    assert_eq!(s.service_name, "smbd");
}

#[test]
fn round_trip_shares() {
    let prefs = tmp_prefs();
    let s = SambaSettings {
        enabled: true,
        username: "alice".into(),
        has_password: true,
        shares: vec![
            share("docs", "/mnt/docs", SambaShareAuth::Password, false),
            share("public", "/mnt/public", SambaShareAuth::Guest, true),
        ],
        service_name: "smbd".into(),
        service_active: true,
        service_enabled: false,
    };
    set_samba_settings(&prefs, &s).unwrap();
    let got = get_samba_settings(&prefs);
    assert_eq!(got, s);
}

// ----- sanitize_share_name -----

#[test]
fn sanitize_keeps_safe_characters() {
    assert_eq!(sanitize_share_name("photos"), "photos");
    assert_eq!(sanitize_share_name("My-Share_2.v2"), "My-Share_2.v2");
}

#[test]
fn sanitize_maps_spaces_and_specials_to_underscore() {
    assert_eq!(sanitize_share_name("my photos"), "my_photos");
    assert_eq!(sanitize_share_name("a/b\\c"), "a_b_c");
    // Pure-CJK names map to underscores which the edge trim then removes —
    // callers fall back to "share" (same as Go).
    assert_eq!(sanitize_share_name("照片库"), "");
}

#[test]
fn sanitize_trims_brackets_and_edges() {
    assert_eq!(sanitize_share_name("[photos]"), "photos");
    assert_eq!(sanitize_share_name("-photos-"), "photos");
    assert_eq!(sanitize_share_name(".photos."), "photos");
}

#[test]
fn sanitize_caps_length_at_32() {
    let out = sanitize_share_name(&"a".repeat(64));
    assert_eq!(out.chars().count(), 32);
}

#[test]
fn sanitize_empty_stays_empty() {
    assert_eq!(sanitize_share_name(""), "");
    assert_eq!(sanitize_share_name("   "), "");
    assert_eq!(sanitize_share_name("[]"), "");
}

// ----- render_smb_conf -----

#[test]
fn render_disabled_has_no_share_sections() {
    let conf = render_smb_conf(
        &settings(
            false,
            vec![share("x", "/mnt/x", SambaShareAuth::Guest, true)],
        ),
        NO_VFS,
        |_| false,
    );
    assert!(conf.starts_with("# Managed by PlainNAS."));
    assert!(conf.contains("[global]"));
    assert!(!conf.contains("[x]"));
}

#[test]
fn render_guest_readonly_share() {
    let conf = render_smb_conf(
        &settings(
            true,
            vec![share("photos", "/mnt/photos", SambaShareAuth::Guest, true)],
        ),
        NO_VFS,
        |_| false,
    );
    assert!(conf.contains("[photos]\n  path = /mnt/photos"));
    assert!(conf.contains("  guest ok = yes\n  guest only = yes\n"));
    assert!(conf.contains("  read only = yes\n"));
    assert!(conf.contains("  force user = nas\n"));
    assert!(!conf.contains("valid users"));
}

#[test]
fn render_password_readwrite_share() {
    let conf = render_smb_conf(
        &settings(
            true,
            vec![share(
                "private",
                "/mnt/private",
                SambaShareAuth::Password,
                false,
            )],
        ),
        NO_VFS,
        |_| false,
    );
    assert!(conf.contains("  guest ok = no\n"));
    assert!(conf.contains("  valid users = nas\n"));
    assert!(conf.contains("  read only = no\n"));
}

#[test]
fn render_fruit_with_xattr_uses_streams_xattr() {
    let caps = VfsCaps {
        fruit: true,
        catia: true,
        streams_xattr: true,
    };
    let conf = render_smb_conf(
        &settings(
            true,
            vec![share("media", "/mnt/media", SambaShareAuth::Guest, false)],
        ),
        caps,
        |_| true,
    );
    assert!(conf.contains("  fruit:aapl = yes\n"));
    assert!(conf.contains("  vfs objects = catia fruit streams_xattr\n"));
    assert!(conf.contains("  ea support = yes\n"));
    assert!(conf.contains("  store dos attributes = yes\n"));
    assert!(conf.contains("  fruit:metadata = stream\n"));
    assert!(conf.contains("  fruit:resource = stream\n"));
    assert!(conf.contains("  fruit:posix_rename = yes\n"));
}

#[test]
fn render_fruit_without_xattr_falls_back_to_sidecar_files() {
    let caps = VfsCaps {
        fruit: true,
        catia: false,
        streams_xattr: true,
    };
    let conf = render_smb_conf(
        &settings(
            true,
            vec![share("usb", "/mnt/usb1", SambaShareAuth::Guest, true)],
        ),
        caps,
        |_| false,
    );
    assert!(conf.contains("  vfs objects = fruit\n"));
    assert!(conf.contains("  fruit:metadata = netatalk\n"));
    assert!(conf.contains("  fruit:resource = file\n"));
    assert!(!conf.contains("streams_xattr"));
    assert!(!conf.contains("ea support"));
}

#[test]
fn render_xattr_probe_is_per_share() {
    let caps = VfsCaps {
        fruit: true,
        catia: false,
        streams_xattr: true,
    };
    let conf = render_smb_conf(
        &settings(
            true,
            vec![
                share("a", "/mnt/a", SambaShareAuth::Guest, true),
                share("b", "/mnt/b", SambaShareAuth::Guest, true),
            ],
        ),
        caps,
        |path| path == "/mnt/a",
    );
    let a = conf
        .split("[a]")
        .nth(1)
        .unwrap()
        .split("[b]")
        .next()
        .unwrap();
    let b = conf.split("[b]").nth(1).unwrap();
    assert!(a.contains("streams_xattr"));
    assert!(b.contains("fruit:metadata = netatalk"));
}

#[test]
fn render_dedupes_share_names_case_insensitively() {
    let conf = render_smb_conf(
        &settings(
            true,
            vec![
                share("Media", "/mnt/one", SambaShareAuth::Guest, true),
                share("media", "/mnt/two", SambaShareAuth::Guest, true),
            ],
        ),
        NO_VFS,
        |_| false,
    );
    assert!(conf.contains("[Media]\n"));
    assert!(conf.contains("[media-2]\n"));
}

#[test]
fn render_empty_name_becomes_share() {
    let conf = render_smb_conf(
        &settings(
            true,
            vec![share("  ", "/mnt/x", SambaShareAuth::Guest, true)],
        ),
        NO_VFS,
        |_| false,
    );
    assert!(conf.contains("[share]\n"));
}

// ----- validate_shares -----

#[test]
fn validate_disabled_allows_anything() {
    assert!(validate_shares(false, &[]).is_ok());
    assert!(validate_shares(false, &[share("x", "", SambaShareAuth::Guest, true)]).is_ok());
}

#[test]
fn validate_enabled_requires_shares() {
    let err = validate_shares(true, &[]).unwrap_err().to_string();
    assert_eq!(err, "no shares configured");
}

#[test]
fn validate_rejects_empty_path() {
    let err = validate_shares(true, &[share("x", "  ", SambaShareAuth::Guest, true)])
        .unwrap_err()
        .to_string();
    assert_eq!(err, "share path is empty");
}

#[test]
fn validate_creates_missing_dir_and_accepts_it() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("fresh/nested");
    assert!(
        validate_shares(
            true,
            &[share(
                "x",
                target.to_str().unwrap(),
                SambaShareAuth::Guest,
                true
            )]
        )
        .is_ok()
    );
    assert!(target.is_dir());
}

#[test]
fn validate_rejects_file_path() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notadir");
    std::fs::write(&file, b"x").unwrap();
    // MkdirAll on an existing file errors (Go propagates the same mkdir
    // error rather than reaching the is-dir check).
    assert!(
        validate_shares(
            true,
            &[share(
                "x",
                file.to_str().unwrap(),
                SambaShareAuth::Guest,
                true
            )]
        )
        .is_err()
    );
}

// ----- service status (non-systemd host) -----

#[test]
fn service_status_without_systemd_is_zero() {
    // On hosts without a loaded smbd/samba/smb unit this must return the
    // zero status rather than error (macOS dev machines always hit this).
    let s = get_service_status();
    if detect_systemd_service_name().is_empty() {
        assert_eq!(s, ServiceStatus::default());
    }
}
