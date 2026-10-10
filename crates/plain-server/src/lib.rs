// `MergedObject` derives nest every mounted root type into one auto-trait
// proof, and the contract schema now mounts ~40 of them; the default
// limit overflows long before the roots themselves do.
#![recursion_limit = "512"]

#[cfg(feature = "api")]
pub mod api;
#[cfg(feature = "system")]
pub mod chat_discovery;
#[cfg(feature = "api")]
pub mod chat_service;
#[cfg(feature = "api")]
pub mod discover;
#[cfg(any(feature = "api", feature = "content_api"))]
pub mod dlna_receiver;
#[cfg(any(feature = "system", feature = "content_api"))]
pub mod dlna_sender;
#[cfg(feature = "api")]
pub mod download;
#[cfg(any(feature = "api", feature = "content_api"))]
pub mod feeds;
#[cfg(feature = "api")]
pub mod http_server;
#[cfg(feature = "http_transport")]
pub mod http_transport;
#[cfg(any(feature = "api", feature = "content_api"))]
pub mod link_preview;
#[cfg(feature = "system")]
pub mod storage;
#[cfg(feature = "system")]
pub mod system;
#[cfg(feature = "http_transport")]
pub mod tls_identity;

#[cfg(feature = "image-inference")]
pub use plain_inference as image_inference;
#[cfg(feature = "media")]
pub use plain_media::media;
pub use plain_rs::*;

#[cfg(feature = "content_api")]
pub mod content_api;
#[cfg(any(feature = "api", feature = "content_api"))]
pub mod content_types;

#[cfg(any(feature = "api", feature = "content_api"))]
pub mod pomodoro;

/// Test fixtures whose dir must outlive the helper that created it
/// (`Db::open(dir)`, `ServerState`…) register it here instead of
/// `mem::forget`-ing the TempDir: dirs stay alive for the whole test
/// run and are deleted when the test process exits normally.
#[cfg(test)]
pub(crate) mod test_tempdirs {
    use std::sync::Mutex;

    static RETAINED: Mutex<Vec<tempfile::TempDir>> = Mutex::new(Vec::new());

    pub(crate) fn retain(dir: tempfile::TempDir) {
        static REGISTERED: std::sync::Once = std::sync::Once::new();
        REGISTERED.call_once(|| unsafe {
            libc::atexit(clear);
        });
        RETAINED.lock().unwrap().push(dir);
    }

    extern "C" fn clear() {
        // Dropping the vec runs each TempDir's recursive delete; bail
        // out if a thread still holds the lock at exit.
        if let Ok(mut dirs) = RETAINED.try_lock() {
            dirs.clear();
        }
    }
}

#[cfg(any(feature = "api", feature = "content_api"))]
#[path = "dlna_sender/media_alias.rs"]
pub mod dlna_media_alias;
