use super::*;

#[test]
fn hostname_sanitization_is_shared() {
    assert_eq!(sanitize_hostname("  Foo_. Bar-- "), "foo-bar");
    assert_eq!(sanitize_hostname("!.."), "");
}

#[test]
fn one_schema_declares_capability_operations() {
    let sdl = crate::httpserver::mainschemas::build_schema().sdl();
    for field in [
        "disks:",
        "sessions:",
        "auditEvents(",
        "appUpdate:",
        "sambaSettings:",
        "dlnaRenderers:",
        "audioLyrics(",
        "formatDisk(",
        "setSambaSettings(",
        "setSambaUserPassword(",
        "dlnaCast(",
        "setHostname(",
        "setMountAlias(",
        "setTempValue(",
        "logout:",
        "revokeSession(",
    ] {
        assert!(sdl.contains(field), "missing {field}");
    }
}

#[cfg(feature = "nas")]
#[tokio::test]
async fn unsupported_hardware_uses_the_same_graphql_schema() {
    let state = crate::api::server::test_support::nas_state();
    let response = crate::httpserver::mainschemas::build_schema()
        .execute(
            async_graphql::Request::new("{ app { deviceType } disks { id } }")
                .data(state.ctx.clone()),
        )
        .await;
    let json = serde_json::to_value(response).unwrap();
    assert_eq!(json["data"]["app"]["deviceType"], "NAS");
    assert_eq!(json["errors"][0]["message"], "disk manager unavailable");
}

#[cfg(feature = "nas")]
#[tokio::test]
async fn temp_value_mutation_feeds_shared_zip_store() {
    let state = crate::api::server::test_support::nas_state();
    let response = crate::httpserver::mainschemas::build_schema()
        .execute(
            async_graphql::Request::new(
                "mutation { setTempValue(key: \"zip-test-key\", value: \"[]\") { key value } }",
            )
            .data(state.ctx),
        )
        .await;
    assert!(response.errors.is_empty(), "{:?}", response.errors);
    assert_eq!(
        crate::api::temp_store::take("zip-test-key"),
        Some("[]".to_string())
    );
}
