//! Media source directories (query `mediaSourceDirs` + mutation
//! `setMediaSourceDirs`) — the `media_source_dirs` preference, a JSON
//! array of the directories the media scanner should index.

use crate::prefs::Prefs;
use anyhow::Result;

const KEY: &str = "media_source_dirs";

pub fn get(prefs: &Prefs) -> Vec<String> {
    prefs
        .get::<Vec<String>>(KEY)
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub fn set(prefs: &Prefs, dirs: &[String]) -> Result<()> {
    prefs.set(KEY, dirs)?;
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/unit/media/kv/media_source_dirs.rs"]
mod tests;
