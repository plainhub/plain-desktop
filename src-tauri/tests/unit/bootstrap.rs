use tauri::plugin::Plugin;

#[test]
fn debug_webviews_enable_console_logging_before_frontend_startup() {
    let plugin = super::native_views_plugin::<tauri::Wry>();
    let script = plugin.initialization_script();
    if cfg!(debug_assertions) {
        assert_eq!(script.as_deref(), Some("window.__PLAIN_LOG__ = true;"));
    } else {
        assert_eq!(script, None);
    }
}
