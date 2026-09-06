#[path = "discover/mod.rs"]
pub mod discover;
pub mod http_client;
#[cfg(target_os = "macos")]
pub mod macos_dock;
#[cfg(target_os = "macos")]
pub mod macos_menu;
pub mod media_preview_pool;
pub mod notification;
pub mod reveal;
pub mod screen_capture;
pub mod updater;
pub mod webview_creation;
pub mod window;
pub mod ws_proxy;

pub use http_client::HttpClient;
