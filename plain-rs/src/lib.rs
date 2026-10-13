// Lets test fixtures that dependent crates mount via `#[path]` name
// this crate explicitly (`plain_rs::…`) instead of `crate::`, which
// would resolve to the *mounting* crate.
extern crate self as plain_rs;

#[cfg(feature = "chat")]
pub mod chat;
#[cfg(feature = "crypto")]
pub mod crypto;
#[cfg(any(feature = "chat", feature = "library", feature = "sqlite_browse"))]
pub mod db;
#[cfg(feature = "library")]
pub mod enums;
#[cfg(feature = "events")]
pub mod events;
#[cfg(feature = "library")]
pub mod media;
#[cfg(feature = "library")]
pub mod notes;
#[cfg(feature = "prefs")]
pub mod prefs;
#[cfg(feature = "tls")]
pub mod tls;
#[cfg(feature = "sqlite_browse")]
pub mod sqlite_browse {
    pub use crate::db::browse::*;
}
#[cfg(any(
    feature = "linux-platform",
    feature = "linux-system",
    feature = "unix-platform",
    feature = "storage-platform"
))]
pub mod platform;
pub mod utils;
#[cfg(feature = "crypto")]
pub use crate::crypto::*;
pub use crate::utils::*;
/// Test fixtures whose dir must outlive the helper that created it
/// (`Db::open(dir)`, `ServerState`…) register it here instead of
/// `mem::forget`-ing the TempDir: dirs stay alive for the whole test
/// run and are deleted when the test process exits normally.
///
/// Also reachable from dependent crates' test binaries through the
/// `test-seams` feature, which is what shared `#[path]`-mounted
/// fixtures need.
#[cfg(any(test, feature = "test-seams"))]
pub mod test_tempdirs {
    use std::sync::Mutex;

    static RETAINED: Mutex<Vec<tempfile::TempDir>> = Mutex::new(Vec::new());

    pub fn retain(dir: tempfile::TempDir) {
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

#[cfg(feature = "library")]
pub mod favorites;

#[cfg(feature = "library")]
pub mod tags;

#[cfg(feature = "library")]
pub mod clipboard;

#[cfg(feature = "library")]
pub mod error;

#[cfg(any(
    feature = "chat",
    feature = "filesystem",
    feature = "services",
    feature = "library"
))]
pub mod files;

#[cfg(any(feature = "ble", feature = "mdns", feature = "crypto"))]
pub mod protocols;

#[cfg(feature = "ble-native")]
pub mod ffi;

#[cfg(feature = "chat")]
pub mod discovery;

#[cfg(feature = "chat")]
pub mod peers;

#[cfg(feature = "chat")]
pub mod transport;

#[cfg(feature = "feeds")]
pub mod feeds;

pub use utils::hostname::platform_hostname;
