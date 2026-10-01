//! Unit tests for the `crate::prefs` global glue — the storage engine
//! itself is covered by plain-rs `tests/unit/prefs*.rs`; here we only
//! lock the set_global/get_default contract. Compiled as the `tests`
//! child module via `#[cfg(test)] #[path]` in `src/prefs.rs`.
use super::*;

fn tmp_prefs() -> (std::path::PathBuf, Prefs) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "plain-nas-prefs-glue-{}-{nanos}",
        std::process::id()
    ));
    let path = default_path(&dir);
    (dir, Prefs::load(&path).unwrap())
}

#[test]
fn default_path_is_system_prefs_json() {
    assert_eq!(
        default_path(std::path::Path::new("/data")),
        std::path::PathBuf::from("/data/system_prefs.json")
    );
}

#[test]
fn reexported_engine_roundtrips_through_the_glue_type() {
    let (_dir, prefs) = tmp_prefs();
    prefs.set("device_name", "box").unwrap();
    assert_eq!(
        prefs.get::<String>("device_name").unwrap().as_deref(),
        Some("box")
    );
}
