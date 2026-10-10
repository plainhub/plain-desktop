use serde::Deserialize;
use std::sync::Mutex;
use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::{App, AppHandle, Manager, Wry};

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MenuLabels {
    about: String,
    quit: String,
    services: String,
    hide: String,
    hide_others: String,
    show_all: String,
    file: String,
    new_window: String,
    edit: String,
    undo: String,
    redo: String,
    cut: String,
    copy: String,
    paste: String,
    select_all: String,
    view: String,
    toggle_devtools: String,
    window: String,
    minimize: String,
    maximize: String,
    fullscreen: String,
    close_window: String,
}

pub struct MenuState(Mutex<MenuLabels>);

fn store_labels(app: &AppHandle, labels: &MenuLabels) {
    if let Some(state) = app.try_state::<MenuState>() {
        *state.0.lock().unwrap() = labels.clone();
    }
}

pub fn about_title(app: &AppHandle) -> String {
    app.try_state::<MenuState>()
        .map(|s| s.0.lock().unwrap().about.clone())
        .unwrap_or_else(|| "About PlainApp".to_string())
}

fn build_menu(app: &AppHandle, t: &MenuLabels) -> tauri::Result<Menu<Wry>> {
    // ── App menu (first menu on macOS is the app name) ────────────────────
    let about = MenuItemBuilder::with_id("about", &t.about).build(app)?;
    let services = PredefinedMenuItem::services(app, Some(&t.services))?;
    let hide = PredefinedMenuItem::hide(app, Some(&t.hide))?;
    let hide_others = PredefinedMenuItem::hide_others(app, Some(&t.hide_others))?;
    let show_all = PredefinedMenuItem::show_all(app, Some(&t.show_all))?;
    let quit = PredefinedMenuItem::quit(app, Some(&t.quit))?;
    let app_submenu = SubmenuBuilder::new(app, "PlainApp")
        .item(&about)
        .separator()
        .item(&services)
        .separator()
        .item(&hide)
        .item(&hide_others)
        .item(&show_all)
        .separator()
        .item(&quit)
        .build()?;

    // ── File menu ─────────────────────────────────────────────────────────
    let new_window = MenuItemBuilder::with_id("new-window", &t.new_window)
        .accelerator("CmdOrCtrl+Shift+N")
        .build(app)?;
    let file_submenu = SubmenuBuilder::new(app, &t.file)
        .item(&new_window)
        .build()?;

    // ── Edit menu (standard items so cut/copy/paste/undo work) ───────────
    let edit_submenu = SubmenuBuilder::new(app, &t.edit)
        .item(&PredefinedMenuItem::undo(app, Some(&t.undo))?)
        .item(&PredefinedMenuItem::redo(app, Some(&t.redo))?)
        .separator()
        .item(&PredefinedMenuItem::cut(app, Some(&t.cut))?)
        .item(&PredefinedMenuItem::copy(app, Some(&t.copy))?)
        .item(&PredefinedMenuItem::paste(app, Some(&t.paste))?)
        .item(&PredefinedMenuItem::select_all(app, Some(&t.select_all))?)
        .build()?;

    // ── View menu (Toggle Developer Tools lets users inspect console errors) ──
    let toggle_devtools = MenuItemBuilder::with_id("toggle-devtools", &t.toggle_devtools)
        .accelerator("CmdOrCtrl+Option+I")
        .build(app)?;
    let view_submenu = SubmenuBuilder::new(app, &t.view)
        .item(&toggle_devtools)
        .build()?;

    // ── Window menu ───────────────────────────────────────────────────────
    // Fullscreen uses a fixed label that reads the same whether the window is
    // fullscreen or not, honoring the translated wording (e.g. Enter/Exit Full Screen).
    let toggle_fullscreen = MenuItemBuilder::with_id("toggle-fullscreen", &t.fullscreen)
        .accelerator("Ctrl+Cmd+F")
        .build(app)?;
    let window_submenu = SubmenuBuilder::new(app, &t.window)
        .item(&PredefinedMenuItem::minimize(app, Some(&t.minimize))?)
        .item(&PredefinedMenuItem::maximize(app, Some(&t.maximize))?)
        .item(&toggle_fullscreen)
        .separator()
        .item(&PredefinedMenuItem::close_window(
            app,
            Some(&t.close_window),
        )?)
        .build()?;

    MenuBuilder::new(app)
        .item(&app_submenu)
        .item(&file_submenu)
        .item(&edit_submenu)
        .item(&view_submenu)
        .item(&window_submenu)
        .build()
}

fn apply_menu(app: &AppHandle, t: &MenuLabels) -> tauri::Result<()> {
    store_labels(app, t);
    let menu = build_menu(app, t)?;
    app.set_menu(menu)?;
    Ok(())
}

#[tauri::command]
pub fn set_menu_locale(app: AppHandle, labels: MenuLabels) -> tauri::Result<()> {
    apply_menu(&app, &labels)
}

pub fn setup(app: &mut App) -> tauri::Result<()> {
    let labels = MenuLabels {
        about: "About PlainApp".to_string(),
        quit: "Quit PlainApp".to_string(),
        services: "Services".to_string(),
        hide: "Hide PlainApp".to_string(),
        hide_others: "Hide Others".to_string(),
        show_all: "Show All".to_string(),
        file: "File".to_string(),
        new_window: "New Window".to_string(),
        edit: "Edit".to_string(),
        undo: "Undo".to_string(),
        redo: "Redo".to_string(),
        cut: "Cut".to_string(),
        copy: "Copy".to_string(),
        paste: "Paste".to_string(),
        select_all: "Select All".to_string(),
        view: "View".to_string(),
        toggle_devtools: "Toggle Developer Tools".to_string(),
        window: "Window".to_string(),
        minimize: "Minimize".to_string(),
        maximize: "Zoom".to_string(),
        fullscreen: "Enter Full Screen".to_string(),
        close_window: "Close Window".to_string(),
    };
    app.manage(MenuState(Mutex::new(labels.clone())));
    apply_menu(app.handle(), &labels)?;

    // ── Dock right-click menu ─────────────────────────────────────────────
    crate::commands::macos_dock::setup(app.handle().clone());

    // ── Menu event handler ────────────────────────────────────────────────
    app.on_menu_event(|app, event| {
        let id = event.id().as_ref();
        match id {
            "new-window" => {
                crate::commands::window::create_window(app, "/".to_string());
            }
            "about" => {
                crate::commands::window::open_about(app);
            }
            "toggle-devtools" => {
                let windows = app.webview_windows();
                let target = windows
                    .values()
                    .find(|win| win.is_focused().unwrap_or(false))
                    .or_else(|| windows.get("main"));
                if let Some(win) = target {
                    if win.is_devtools_open() {
                        win.close_devtools();
                    } else {
                        win.open_devtools();
                    }
                }
            }
            "toggle-fullscreen" => {
                let windows = app.webview_windows();
                let target = windows
                    .values()
                    .find(|win| win.is_focused().unwrap_or(false))
                    .or_else(|| windows.get("main"));
                if let Some(win) = target {
                    let is_fullscreen = win.is_fullscreen().unwrap_or(false);
                    let _ = win.set_fullscreen(!is_fullscreen);
                }
            }
            _ => {}
        }
    });

    Ok(())
}
