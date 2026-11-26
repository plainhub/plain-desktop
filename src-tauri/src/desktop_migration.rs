use plain_libs::rusqlite::{Connection, OpenFlags};
use plain_rs::infrastructure::data_paths::DataPaths;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindowBuilder, webview::PageLoadEvent};

const LABEL: &str = "desktop-migration";
const PAGE: &str = "desktop-migration.html";
const WAITING: u8 = 0;
const RUNNING: u8 = 1;
const FAILED: u8 = 2;

pub(crate) struct Startup {
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub log_file: Option<PathBuf>,
}

pub(crate) fn pending(data: &Path) -> bool {
    match data.join("local_chat.db").symlink_metadata() {
        Err(error) if error.kind() == ErrorKind::NotFound => return false,
        Err(_) => return true,
        Ok(_) => {}
    }
    let complete = Connection::open_with_flags(
        DataPaths::new(data).database(),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .and_then(|db| {
        db.query_row(
            "SELECT value FROM key_values WHERE key='desktop_chat_upgrade_4_0_0'",
            [],
            |row| row.get::<_, String>(0),
        )
    });
    !complete.is_ok_and(|value| value == "complete")
}

fn page(url: &Url, dev: Option<&Url>) -> bool {
    if url.path() != "/desktop-migration.html" || url.fragment().is_some() {
        return false;
    }
    let local = matches!(
        (url.scheme(), url.host_str()),
        ("tauri", Some("localhost")) | ("http" | "https", Some("tauri.localhost"))
    ) && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none();
    local || dev.is_some_and(|dev| url.origin() == dev.origin())
}

pub(crate) fn focus(app: &AppHandle) -> bool {
    let Some(window) = app.get_webview_window(LABEL) else {
        return false;
    };
    tauri::async_runtime::spawn_blocking(move || {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    });
    true
}

struct Migration {
    app: AppHandle,
    startup: Startup,
    state: AtomicU8,
}

impl Migration {
    fn start(self: &Arc<Self>, expected: u8) {
        if self
            .state
            .compare_exchange(expected, RUNNING, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let this = self.clone();
        tauri::async_runtime::spawn(async move {
            let worker = this.clone();
            let result = tauri::async_runtime::spawn_blocking(move || {
                tauri::async_runtime::block_on(async {
                    crate::bootstrap::open_desktop_core(
                        &worker.startup.data_dir,
                        &worker.startup.cache_dir,
                    )
                })
            })
            .await
            .unwrap_or_else(|error| Err(error.to_string()));
            let handle = this.app.clone();
            if let Err(error) = handle.run_on_main_thread(move || this.present(result)) {
                log::error!("Desktop migration result could not reach the main thread: {error}");
            }
        });
    }

    fn present(&self, result: Result<plain_server::server::ContentServer, String>) {
        let Some(window) = self.app.get_webview_window(LABEL) else {
            self.app.exit(1);
            return;
        };
        match result {
            Ok(core) => {
                if let Err(error) = crate::bootstrap::finish_startup(
                    &self.app,
                    Arc::new(core),
                    self.startup.log_file.clone(),
                ) {
                    log::error!("Desktop startup after migration failed: {error}");
                    self.app.exit(1);
                    return;
                }
                let handle = self.app.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    if let Err(error) = crate::bootstrap::open_main_window(&handle) {
                        log::error!("Desktop main window failed to open: {error}");
                        handle.exit(1);
                        return;
                    }
                    let _ = window.destroy();
                });
            }
            Err(error) => {
                log::error!("Desktop migration blocked startup: {error}");
                self.state.store(FAILED, Ordering::Release);
                tauri::async_runtime::spawn_blocking(move || {
                    let _ = window.eval("document.documentElement.dataset.state = 'failed'");
                });
            }
        }
    }
}

pub(crate) fn show(app: &AppHandle, startup: Startup) -> tauri::Result<()> {
    let state = Arc::new(Migration {
        app: app.clone(),
        startup,
        state: AtomicU8::new(WAITING),
    });
    let nav = state.clone();
    let loaded = state.clone();
    let dev = if cfg!(debug_assertions) {
        app.config().build.dev_url.clone()
    } else {
        None
    };
    let dev_page = dev.clone();
    let popup = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App(PAGE.into()))
        .title("PlainApp")
        .inner_size(420.0, 240.0)
        .center()
        .resizable(false)
        .maximizable(false)
        .on_navigation(move |url| {
            if !page(url, dev.as_ref()) {
                return false;
            }
            match url.query() {
                None => true,
                Some("retry=1") => {
                    nav.start(FAILED);
                    false
                }
                _ => false,
            }
        })
        .on_page_load(move |_, event| {
            if event.event() == PageLoadEvent::Finished
                && page(event.url(), dev_page.as_ref())
                && event.url().query().is_none()
            {
                loaded.start(WAITING);
            }
        })
        .build()?;
    popup.on_window_event({
        let app = app.clone();
        move |event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                app.exit(0);
            }
        }
    });
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/desktop_migration.rs"]
mod tests;
