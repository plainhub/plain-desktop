#[cfg(feature = "chat")]
pub mod app_files;
#[cfg(feature = "ble")]
pub mod ble;
#[cfg(feature = "chat")]
pub mod chat;
#[cfg(feature = "crypto")]
pub mod crypto;
#[cfg(any(feature = "chat", feature = "library", feature = "sqlite_browse"))]
pub mod db;
#[cfg(feature = "library")]
pub mod enums;
#[cfg(feature = "filesystem")]
pub mod filesystem;
#[cfg(feature = "services")]
pub mod image_editor;
#[cfg(feature = "library")]
pub mod library;
#[cfg(feature = "mdns")]
pub mod mdns;
#[cfg(feature = "library")]
pub mod notes;
#[cfg(feature = "prefs")]
pub mod prefs;
#[cfg(feature = "services")]
pub mod shares;
#[cfg(feature = "tls")]
pub mod tls;
#[cfg(feature = "services")]
pub mod uploads;
#[cfg(feature = "library")]
pub mod video_progress;
#[cfg(feature = "events")]
pub mod ws_event;
#[cfg(feature = "sqlite_browse")]
pub mod sqlite_browse {
    pub use crate::db::browse::*;
}
pub mod utils;
#[cfg(feature = "crypto")]
pub use crypto::*;
pub use utils::*;
#[cfg(feature = "crypto")]
pub mod ws_frame;
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
