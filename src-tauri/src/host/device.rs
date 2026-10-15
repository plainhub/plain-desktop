//! The capabilities the desktop shell can actually drive. Persisted state
//! goes through the shared `plain_rs::prefs::Prefs`; nothing here touches
/// storage.

/// What this shell can actually drive, in the wire form the schema parses.
/// The host bridge reports the same list, so the UI must not see a section
/// from one surface and not the other.
pub fn desktop_capabilities() -> Vec<&'static str> {
    let mut capabilities = vec![
        "DOC_PREVIEW",
        "IMAGE_EDITOR",
        "NOTIFICATIONS",
        "NOTES",
        "FEEDS",
        "POMODORO",
        "APP_UPDATES",
    ];
    capabilities.extend(plain_server::services::files::media_action_io::capabilities());
    capabilities
}

#[cfg(test)]
mod tests {
    use super::desktop_capabilities;
    use plain_server::graphql::resolvers::app::types::Capability;

    /// The host bridge reports these as wire strings; the schema parses the
    /// same list. A name one side cannot read leaves the UI with a section
    /// it cannot drive.
    #[test]
    fn every_desktop_capability_parses() {
        for name in desktop_capabilities() {
            assert!(
                Capability::parse(name).is_some(),
                "{name} is reported but the schema cannot parse it"
            );
        }
    }

    #[test]
    fn the_desktop_list_is_not_empty() {
        assert!(
            !desktop_capabilities().is_empty(),
            "an empty capability list hides every optional section"
        );
    }
}
