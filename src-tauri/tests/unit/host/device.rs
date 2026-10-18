use super::*;
use plain_server::graphql::resolvers::app::types::Capability;

#[test]
fn every_declared_capability_is_unique_and_understood_by_the_main_schema() {
    let capabilities = desktop_capabilities();
    let distinct = capabilities
        .iter()
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(capabilities.len(), distinct.len());
    for name in capabilities {
        assert!(Capability::parse(name).is_some(), "{name}");
    }
}

#[test]
fn file_action_support_matches_the_implemented_os_primitives() {
    let capabilities = desktop_capabilities();
    assert!(capabilities.contains(&"FILE_EXPORT"));
    assert!(capabilities.contains(&"FILE_REVEAL"));
    assert_eq!(
        capabilities.contains(&"FILE_CLIPBOARD"),
        cfg!(target_os = "macos")
    );
}

#[test]
fn discovery_is_supported_independently_of_optional_file_sharing_services() {
    let capabilities = desktop_capabilities();
    assert!(capabilities.contains(&"PEER_DISCOVERY"));
    assert!(capabilities.contains(&"FILE_BROWSER"));
    assert!(!capabilities.contains(&"BACKGROUND_SERVICE"));
    assert!(!capabilities.contains(&"LAUNCHER_SHORTCUTS"));
}

#[test]
fn text_clipboard_is_available_through_the_native_platform_adapter() {
    assert!(desktop_capabilities().contains(&"CLIPBOARD"));
}
