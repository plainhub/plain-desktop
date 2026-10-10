use super::*;
use std::path::Path;

fn peer_sdl_path() -> std::path::PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../plain-rs"))
        .join("../schema/peer_scheme.graphql")
}

#[test]
fn peer_sdl_matches_committed_artifact() {
    let committed = std::fs::read_to_string(peer_sdl_path()).expect("peer SDL artifact");
    assert_eq!(build_schema().sdl(), committed);
}

#[test]
#[ignore]
fn export_peer_schema_sdl() {
    let path = peer_sdl_path();
    std::fs::create_dir_all(path.parent().expect("schema directory"))
        .expect("create schema directory");
    std::fs::write(path, build_schema().sdl()).expect("write peer SDL");
}
