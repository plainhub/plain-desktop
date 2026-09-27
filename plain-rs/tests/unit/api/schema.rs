use super::*;

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
