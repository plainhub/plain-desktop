pub use plain_rs::prefs::Prefs;

pub fn get_capture_shortcut(prefs: &Prefs) -> Option<String> {
    let value: String = prefs.get_or("capture_shortcut", String::new());
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

pub fn set_capture_shortcut(prefs: &Prefs, value: Option<&str>) {
    let _ = prefs.set("capture_shortcut", value.unwrap_or(""));
}
