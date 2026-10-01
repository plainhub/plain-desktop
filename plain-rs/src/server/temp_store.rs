//! In-memory temp values (plain-app `TempHelper` is in-memory too) —
//! cross-screen UI handoff data, never persisted. The web flow writes a
//! value with the `setTempValue` mutation and reads it back once from
//! `/zip?tmp=<key>`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

fn map() -> &'static Mutex<HashMap<String, String>> {
    static MAP: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn set(key: &str, value: &str) {
    map()
        .lock()
        .unwrap()
        .insert(key.to_string(), value.to_string());
}

/// Read one value. `take` removes it (single-consumer handoff).
pub fn take(key: &str) -> Option<String> {
    map().lock().unwrap().remove(key)
}

#[cfg(test)]
pub fn get(key: &str) -> Option<String> {
    map().lock().unwrap().get(key).cloned()
}

#[cfg(test)]
#[path = "../../tests/unit/api/temp_store.rs"]
mod tests;
