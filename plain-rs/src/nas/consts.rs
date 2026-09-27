//! Application-wide constants and runtime paths.

use std::path::PathBuf;

#[derive(Clone)]
pub struct AppPaths {
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub config_path: PathBuf,
    pub tls_cert: PathBuf,
    pub tls_key: PathBuf,
}

impl AppPaths {
    /// Test seam: pin `PLAIN_NAS_DATA_DIR` for the whole test binary to a
    /// fixed, never-deleted directory. Tests that pointed this var at a
    /// per-test `TempDir` poisoned the process-global media search index:
    /// whichever value the env var held when `search_index::global()` first
    /// initialized rooted the index there, and the owning test's `TempDir`
    /// cleanup then deleted the index directory out from under every later
    /// test (empty search results / IO errors).
    #[doc(hidden)]
    pub fn pin_test_data_dir() -> PathBuf {
        static PINNED: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        PINNED
            .get_or_init(|| {
                let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-data-dir");
                std::fs::create_dir_all(&dir).expect("mkdir target/test-data-dir");
                // SAFETY: test-only; writes the same value on every call.
                unsafe { std::env::set_var("PLAIN_NAS_DATA_DIR", &dir) };
                dir
            })
            .clone()
    }

    pub fn detect() -> Self {
        let data_dir = std::env::var("PLAIN_NAS_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(DATA_DIR));
        let cache_dir = std::env::var("PLAIN_NAS_CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home_cache_dir());
        let config_path = std::env::var("PLAIN_NAS_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(ETC_MAIN_CONFIG));
        let tls_cert = std::env::var("PLAIN_NAS_TLS_CERT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(ETC_TLS_SERVER_PEM));
        let tls_key = std::env::var("PLAIN_NAS_TLS_KEY")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(ETC_TLS_SERVER_KEY));
        Self {
            data_dir,
            cache_dir,
            config_path,
            tls_cert,
            tls_key,
        }
    }
}

/// Default cache directory: `~/.plainnas/cache`
fn home_cache_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    PathBuf::from(home).join(".plainnas").join("cache")
}

pub const ETC_MAIN_CONFIG: &str = "/etc/plainnas/config.toml";
pub const ETC_TLS_SERVER_PEM: &str = "/etc/plainnas/tls.pem";
pub const ETC_TLS_SERVER_KEY: &str = "/etc/plainnas/tls.key";

pub const EVENT_MEDIA_SCAN_PROGRESS: &str = "media:scan:progress";
pub const EVENT_FILE_TASK_PROGRESS: &str = "file:task:progress";
pub const EVENT_DLNA_RENDERER_FOUND: &str = "dlna:renderer:found";
pub const EVENT_DLNA_DISCOVERY_DONE: &str = "dlna:discovery:done";
pub const EVENT_DISK_FORMAT_DONE: &str = "disk:format:done";
/// Chat + pairing push events. Payload: `{"msgType": <phone-protocol
/// number>, "payload": <string-or-object>}` — forwarded verbatim to WS
/// clients by the hub.
pub const EVENT_CHAT: &str = "chat:event";

pub static DATA_DIR: &str = "/var/lib/plainnas";
