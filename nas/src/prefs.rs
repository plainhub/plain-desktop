//! Process-wide preferences glue over the shared `plain_rs::prefs`
//! engine (`<data_dir>/prefs.json`, one flat string→JSON map). The
//! engine — atomic tmp+rename writes, pretty-printing, key-sorted
//! entries — and the process-global accessor both live in plain-rs;
//! this module only keeps the `crate::prefs::…` call sites stable.
//! User settings, device identity and small app state live here;
//! media rows / sessions / events live in the fjall store; the user
//! library (audio queue/playlists/history, tags, favorite folders,
//! chat) lives in SQLite via plain-rs.

pub use plain_rs::prefs::{Prefs, default_path, set_global};

#[cfg(test)]
#[path = "../tests/unit/prefs.rs"]
mod tests;
