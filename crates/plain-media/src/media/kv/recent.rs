//! Recent files tracking — the `recent_files` preference, an MRU list of
//! file paths (max 500) as a JSON array.

use crate::prefs::Prefs;

const RECENT_FILES_KEY: &str = "recent_files";
const MAX_RECENT: usize = 500;

/// Add a file path to the recent files list (most-recent first).
/// Deduplicates and trims to 500 entries.
pub fn add_recent_file(prefs: &Prefs, path: &str) {
    let mut stored: Vec<String> = load_recent(prefs);

    // Remove existing occurrences.
    stored.retain(|p| p != path);

    // Prepend new path.
    stored.insert(0, path.to_string());

    // Trim to MAX_RECENT.
    stored.truncate(MAX_RECENT);

    let _ = prefs.set(RECENT_FILES_KEY, &stored);
}

/// Get recent file paths (most-recent first), up to `limit` entries.
pub fn get_recent_files(prefs: &Prefs, limit: usize) -> Vec<String> {
    let stored = load_recent(prefs);
    if limit > 0 && stored.len() > limit {
        stored[..limit].to_vec()
    } else {
        stored
    }
}

fn load_recent(prefs: &Prefs) -> Vec<String> {
    prefs
        .get::<Vec<String>>(RECENT_FILES_KEY)
        .ok()
        .flatten()
        .unwrap_or_default()
}
