use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, RwLock};

use objc2::runtime::{AnyObject, ClassBuilder, Sel};
use tauri::Manager;
use objc2::{msg_send, sel, ClassType};
use objc2_app_kit::{NSApplication, NSMenu, NSMenuItem};
use objc2_foundation::{MainThreadMarker, NSString};

static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

/// Maps window label → display title ("Local" | device name).
static WINDOW_TITLES: OnceLock<RwLock<HashMap<String, String>>> = OnceLock::new();

/// Snapshot of window labels indexed by NSMenuItem.tag, for focusWindow: handler.
static WINDOW_SNAPSHOT: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

fn window_titles() -> &'static RwLock<HashMap<String, String>> {
    WINDOW_TITLES.get_or_init(|| RwLock::new(HashMap::new()))
}

fn window_snapshot() -> &'static Mutex<Vec<String>> {
    WINDOW_SNAPSHOT.get_or_init(|| Mutex::new(Vec::new()))
}

/// Called by the Tauri command to update the display name for a window.
pub fn set_window_device_name(label: &str, name: &str) {
    if let Ok(mut m) = window_titles().write() {
        m.insert(label.to_string(), name.to_string());
    }
}

/// Called when a window is destroyed to remove its entry.
pub fn remove_window_device_name(label: &str) {
    if let Ok(mut m) = window_titles().write() {
        m.remove(label);
    }
}

fn register_action_class() -> &'static objc2::runtime::AnyClass {
    static CLASS: OnceLock<&'static objc2::runtime::AnyClass> = OnceLock::new();
    CLASS.get_or_init(|| {
        use objc2::runtime::NSObject;
        let mut builder = ClassBuilder::new(c"PlainAppDockTarget", NSObject::class())
            .expect("PlainAppDockTarget should be unique");

        unsafe extern "C-unwind" fn open_new_window(
            _this: *mut AnyObject,
            _cmd: Sel,
            _sender: *mut AnyObject,
        ) {
            if let Some(h) = APP_HANDLE.get() {
                crate::commands::window::new_window(h);
            }
        }

        /// NSMenuItem action: focus the window whose label is stored at snapshot[tag].
        unsafe extern "C-unwind" fn focus_window(
            _this: *mut AnyObject,
            _cmd: Sel,
            sender: *mut AnyObject,
        ) {
            let tag: isize = msg_send![sender, tag];
            if let Some(handle) = APP_HANDLE.get()
                && let Ok(snapshot) = window_snapshot().lock()
                && let Some(label) = snapshot.get(tag as usize)
                && let Some(win) = handle.get_webview_window(label)
            {
                let _ = win.show();
                let _ = win.set_focus();
            }
        }

        /// NSMenuDelegate: called on the main thread just before the dock menu appears.
        /// Rebuilds menu contents dynamically from currently open windows.
        unsafe extern "C-unwind" fn menu_needs_update(
            this: *mut AnyObject,
            _cmd: Sel,
            menu: *mut AnyObject,
        ) {
            let _: () = msg_send![menu, removeAllItems];
            let mtm = unsafe { MainThreadMarker::new_unchecked() };

            let mut snapshot: Vec<String> = Vec::new();

            if let Some(handle) = APP_HANDLE.get() {
                let titles = window_titles().read().ok().map(|m| m.clone()).unwrap_or_default();

                let mut labels: Vec<String> = handle
                    .webview_windows()
                    .keys()
                    .filter(|label| !super::screen_capture::runtime::is_overlay_window_label(label))
                    .cloned()
                    .collect();
                labels.sort(); // "window-{timestamp}" — ascending = creation order

                for label in &labels {
                    let display = titles
                        .get(label)
                        .cloned()
                        .unwrap_or_else(|| "PlainApp".to_string());

                    let title_ns = NSString::from_str(&display);
                    let key_ns = NSString::from_str("");
                    let item: *mut AnyObject = msg_send![NSMenuItem::class(), alloc];
                    let item: *mut AnyObject = msg_send![
                        item, initWithTitle: &*title_ns,
                        action: sel!(focusWindow:),
                        keyEquivalent: &*key_ns
                    ];
                    let tag = snapshot.len() as isize;
                    let _: () = msg_send![item, setTag: tag];
                    let _: () = msg_send![item, setTarget: this];
                    let _: () = msg_send![menu, addItem: item];
                    let _: () = msg_send![item, release];
                    snapshot.push(label.clone());
                }

                if !labels.is_empty() {
                    let sep = NSMenuItem::separatorItem(mtm);
                    let _: () = msg_send![menu, addItem: &*sep];
                }
            }

            // Update snapshot so focusWindow: maps tag → correct label.
            if let Ok(mut s) = window_snapshot().lock() {
                *s = snapshot;
            }

            // "New Window" item always at the bottom.
            let title = NSString::from_str("New Window");
            let key = NSString::from_str("");
            let item: *mut AnyObject = msg_send![NSMenuItem::class(), alloc];
            let item: *mut AnyObject = msg_send![
                item, initWithTitle: &*title,
                action: sel!(openNewWindow:),
                keyEquivalent: &*key
            ];
            let _: () = msg_send![item, setTarget: this];
            let _: () = msg_send![menu, addItem: item];
            let _: () = msg_send![item, release];
        }

        unsafe {
            builder.add_method(
                sel!(openNewWindow:),
                open_new_window as unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            builder.add_method(
                sel!(focusWindow:),
                focus_window as unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            builder.add_method(
                sel!(menuNeedsUpdate:),
                menu_needs_update
                    as unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
        }

        builder.register()
    })
}

/// Sets up the macOS dock right-click menu. Must be called from the main thread.
pub fn setup(handle: tauri::AppHandle) {
    let _ = APP_HANDLE.set(handle);

    unsafe {
        let cls = register_action_class();

        // `new` → +1 retained. Leaked intentionally: lives for the app's lifetime.
        let target: *mut AnyObject = msg_send![cls, new];

        let mtm = MainThreadMarker::new_unchecked();
        let app = NSApplication::sharedApplication(mtm);

        let menu = NSMenu::new(mtm);
        // Set our object as the NSMenuDelegate so menuNeedsUpdate: is called
        // each time the dock menu is about to appear — giving us a dynamic list.
        let _: () = msg_send![&*menu, setDelegate: target];

        // Static fallback: "New Window" is always present even if the delegate
        // method is not invoked (dock menus can behave differently).
        let title = NSString::from_str("New Window");
        let key = NSString::from_str("");
        let item: *mut AnyObject = msg_send![NSMenuItem::class(), alloc];
        let item: *mut AnyObject =
            msg_send![item, initWithTitle: &*title, action: sel!(openNewWindow:), keyEquivalent: &*key];
        let _: () = msg_send![item, setTarget: target];
        menu.addItem(&*(item as *const NSMenuItem));
        let _: () = msg_send![item, release];

        // NSApp retains the dock menu.
        let _: () = msg_send![&*app, setDockMenu: &*menu];
    }
}
