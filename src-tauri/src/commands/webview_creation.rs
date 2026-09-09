#[cfg(target_os = "windows")]
use std::sync::Mutex;

// WebView2 controller creation pumps the Windows message queue while Tauri
// holds its WebContext store lock. A second dynamic webview creation handled
// by that nested pump blocks forever on the same non-reentrant mutex. Keep all
// Plain-owned dynamic WebView2 builders behind one off-main-thread gate.
#[cfg(target_os = "windows")]
static WEBVIEW_CREATION_LOCK: Mutex<()> = Mutex::new(());

pub fn serialized<T>(operation: impl FnOnce() -> T) -> T {
    #[cfg(target_os = "windows")]
    {
        let _guard = WEBVIEW_CREATION_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        operation()
    }
    #[cfg(not(target_os = "windows"))]
    {
        operation()
    }
}
