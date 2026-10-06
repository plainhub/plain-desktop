use super::*;

#[test]
fn client_paths_cannot_escape_the_bundle_root() {
    assert_eq!(
        safe_relative("assets/app.js").unwrap(),
        PathBuf::from("assets/app.js")
    );
    assert_eq!(
        safe_relative("/index.html").unwrap(),
        PathBuf::from("index.html")
    );
    assert_eq!(safe_relative(""), None);
    assert_eq!(safe_relative(".."), None);
    assert_eq!(safe_relative("../secret"), None);
    assert_eq!(safe_relative("assets/../../secret"), None);
    assert_eq!(safe_relative("assets\\..\\..\\secret"), None);
}

#[test]
fn hashed_asset_prefixes_are_immutable_and_the_shell_is_not() {
    assert_eq!(
        cache_control("assets/index-abc123.js"),
        "public, max-age=31536000"
    );
    assert_eq!(cache_control("ficons/note.svg"), "public, max-age=31536000");
    assert_eq!(cache_control("index.html"), "no-cache, no-store");
    assert_eq!(cache_control("favicon.ico"), "no-cache, no-store");
}

#[test]
fn server_time_is_injected_into_the_spa_shell() {
    let html = inject_server_time("<html><head><title>x</title></head></html>", 42);
    assert!(html.starts_with("<html><head><script>window.__SERVER_TIME__=42</script>"));
    // A shell without <head> still gets the bootstrap rather than losing it.
    assert!(
        inject_server_time("<div/>", 7).starts_with("<script>window.__SERVER_TIME__=7</script>")
    );
}

#[test]
fn content_types_cover_the_bundler_output() {
    assert_eq!(content_type("index.html"), "text/html; charset=utf-8");
    assert_eq!(
        content_type("assets/app-abc.js"),
        "application/javascript; charset=utf-8"
    );
    assert_eq!(
        content_type("assets/app-abc.css"),
        "text/css; charset=utf-8"
    );
    assert_eq!(content_type("ficons/note.svg"), "image/svg+xml");
    assert_eq!(content_type("logo.png"), "image/png");
    assert_eq!(content_type("Inter.woff2"), "font/woff2");
    assert_eq!(content_type("noext"), "application/octet-stream");
}
