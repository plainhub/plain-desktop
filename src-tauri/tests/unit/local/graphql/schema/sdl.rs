//! SDL artifact export for the homegrown GraphQL codegen.
//!
//! `src/lib/api/graphql/local-schema.graphql` is the committed contract the
//! frontend document validator and type generator run against. The snapshot
//! test fails whenever the Rust schema drifts from the artifact; regenerate
//! with `cargo test --manifest-path src-tauri/Cargo.toml export_sdl -- --ignored`.

use std::path::{Path, PathBuf};

fn sdl_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/api/graphql/local-schema.graphql")
}

#[test]
fn sdl_snapshot_matches_committed_artifact() {
    let sdl = super::build_schema().sdl();
    let committed = std::fs::read_to_string(sdl_path())
        .unwrap_or_else(|e| panic!("committed SDL artifact missing: {e}"));
    assert_eq!(
        sdl, committed,
        "schema drifted from src/lib/api/graphql/local-schema.graphql — \
         regenerate with: cargo test --manifest-path src-tauri/Cargo.toml export_sdl -- --ignored"
    );
}

#[test]
#[ignore]
fn export_sdl() {
    if let Some(dir) = sdl_path().parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(sdl_path(), super::build_schema().sdl()).unwrap();
}
