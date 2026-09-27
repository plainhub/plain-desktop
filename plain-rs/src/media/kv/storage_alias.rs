//! Persistent user-defined volume aliases — the `volume_alias`
//! preference, a single JSON object mapping volume id → display alias.

use crate::prefs::Prefs;
use anyhow::Result;
use serde_json::{Map, Value};

const KEY: &str = "volume_alias";

pub fn get_map(prefs: &Prefs) -> Map<String, Value> {
    prefs
        .get::<Map<String, Value>>(KEY)
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub fn set_map(prefs: &Prefs, m: &Map<String, Value>) -> Result<()> {
    prefs.set(KEY, m)?;
    Ok(())
}

pub fn set_alias(prefs: &Prefs, id: &str, alias: &str) -> Result<()> {
    let mut m = get_map(prefs);
    let trimmed = alias.trim();
    if trimmed.is_empty() {
        m.remove(id);
    } else {
        m.insert(id.to_string(), Value::String(trimmed.to_string()));
    }
    set_map(prefs, &m)
}
