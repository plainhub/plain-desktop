//! Unit tests for the walkdir file search in `src/search.rs` — the DSL
//! parser tests moved to plain-rs (`tests/unit/utils/search_dsl.rs`).
use super::*;

#[test]
fn search_files_walks_tree() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    std::fs::write(dir.path().join("a.txt"), "x").unwrap();
    std::fs::write(dir.path().join("b.txt"), "y").unwrap();
    std::fs::write(dir.path().join("sub/c.txt"), "z").unwrap();
    std::fs::write(dir.path().join(".hidden"), "z").unwrap();

    let all = search_files("", dir.path(), false);
    // Should NOT include .hidden
    assert!(!all.iter().any(|f| f.path.ends_with(".hidden")));
    // Should include a.txt, b.txt, sub, sub/c.txt
    assert!(all.iter().any(|f| f.path.ends_with("a.txt")));
    assert!(all.iter().any(|f| f.path.ends_with("b.txt")));
    assert!(all.iter().any(|f| f.path.ends_with("sub")));
    assert!(all.iter().any(|f| f.path.ends_with("c.txt")));

    let txt = search_files("a", dir.path(), false);
    assert!(txt.iter().all(|f| f.path.contains("a")));
    assert!(!txt.iter().any(|f| f.path.ends_with("b.txt")));
}
