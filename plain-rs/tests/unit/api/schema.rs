use super::*;
use std::path::Path;

fn desktop_sdl_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/api/graphql/local-schema.graphql")
}

#[test]
fn desktop_sdl_matches_committed_artifact() {
    let committed = std::fs::read_to_string(desktop_sdl_path()).expect("desktop SDL artifact");
    assert_eq!(build_schema().sdl(), committed);
}

#[test]
#[ignore]
fn export_desktop_sdl() {
    std::fs::write(desktop_sdl_path(), build_schema().sdl()).expect("write desktop SDL");
}

/// The desktop schema merges its own roots with the media gql roots;
/// async-graphql panics at build time when two types or two fields claim
/// the same GraphQL name. This locks the merged schema against duplicate
/// wire names (the `DataType` / `Tag` / `tags` collision crashed
/// `yarn dev:tauri` at startup).
#[test]
fn merged_schema_builds_without_duplicate_names() {
    let _schema = build_schema();
}

/// Wire contract spot-checks: the tag surface comes from the media gql
/// roots (single copy) and uses the plain-app `DataType` enum.
#[test]
fn merged_schema_sdl_has_single_tag_surface() {
    let schema = build_schema();
    let sdl = schema.sdl();
    assert_eq!(sdl.matches("enum DataType").count(), 1);
    assert_eq!(sdl.matches("type Tag ").count(), 1);
    assert_eq!(sdl.matches("type TagRelation ").count(), 1);
    assert_eq!(sdl.matches("type ActionResult ").count(), 1);
    assert!(sdl.contains("tags(type: DataType!): [Tag!]!"));
    assert!(sdl.contains("PACKAGE"));
    assert!(sdl.contains("APP_FILE"));
}

#[test]
fn pairing_inputs_match_plain_app_wire_types() {
    let sdl = build_schema().sdl();
    assert!(sdl.contains("cancelPairing(deviceId: ID!): Boolean!"));
    assert!(sdl.contains("id: ID!"));
    assert!(sdl.contains("deviceType: DeviceType!"));
    assert!(sdl.contains("lastSeen: Instant!"));
    assert!(sdl.contains("discoveryMethods: [DiscoveryMethod!]!"));
    assert!(sdl.contains("fromId: ID!"));
    assert!(sdl.contains("timestamp: Long!"));
}

#[test]
fn desktop_schema_exposes_shared_media_fields() {
    let sdl = build_schema().sdl();
    for field in [
        "images(",
        "videos(",
        "audios(",
        "docs(",
        "files(",
        "scanProgress:",
        "startMediaScan(",
        "rebuildMediaIndex(",
    ] {
        assert!(sdl.contains(field), "missing {field}");
    }
}
