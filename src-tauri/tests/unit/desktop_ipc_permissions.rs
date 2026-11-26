use tauri::ipc::Origin;

#[test]
fn desktop_ipc_permissions_match_their_local_windows() {
    let mut context = crate::bootstrap::application_context();
    let acl = context.runtime_authority_mut();
    let local = |command, label| {
        acl.resolve_access(command, label, label, &Origin::Local)
            .is_some()
    };

    for window in [
        "main",
        "window-frontend",
        "about",
        "media-preview-warm",
        "media-preview-1",
    ] {
        assert!(local("native_view_scope", window), "{window} cannot start");
        assert!(
            local("native_graphql", window),
            "{window} cannot load settings"
        );
        assert_eq!(local("install_desktop_update", window), window == "about");
    }
    assert!(local("screen_capture_start", "main"));
    assert!(!local("screen_capture_ready", "main"));

    let overlay = "screen-capture-overlay-1";
    assert!(local("native_view_scope", overlay));
    assert!(local("screen_capture_ready", overlay));
    assert!(!local("native_graphql", overlay));
    assert!(!local("install_desktop_update", overlay));
    assert!(!local("native_view_scope", "desktop-migration"));
    assert!(!local("native_media_report", "media-owner"));

    let remote = Origin::Remote {
        url: "https://example.com/".parse().unwrap(),
    };
    for command in [
        "native_view_scope",
        "native_graphql",
        "install_desktop_update",
    ] {
        assert!(
            acl.resolve_access(command, "about", "about", &remote)
                .is_none()
        );
    }
}
