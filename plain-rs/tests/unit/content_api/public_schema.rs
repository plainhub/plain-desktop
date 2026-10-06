//! The public `/graphql` schema is the network contract. Its root field
//! set and signatures are frozen against `testdata/public-schema.graphqls`:
//! a missing root field silently breaks the web console, and an extra one
//! publishes an internal API to the LAN.

use super::build;
use crate::{db::Db, prefs::Prefs};
use std::sync::Arc;

fn sdl() -> String {
    let db = Arc::new(Db::open(std::path::Path::new(":memory:")).unwrap());
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&dir.path().join("system_prefs.json")).unwrap());
    build(
        Arc::new(crate::content_api::host::Host::default()),
        prefs,
        db,
    )
    .sdl()
}

#[test]
fn public_schema_matches_committed_sdl() {
    let sdl = sdl();
    if std::env::var_os("UPDATE_PUBLIC_SCHEMA").is_some() {
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/testdata/public-schema.graphqls"
            ),
            &sdl,
        )
        .unwrap();
    } else {
        assert_eq!(
            sdl,
            include_str!("../../../testdata/public-schema.graphqls")
        );
    }
}

/// The contract shapes every resolver must keep, spelled out so a rename or
/// an optional-turned-required argument cannot pass the snapshot review.
#[test]
fn public_schema_exposes_the_contract_it_has_to_serve() {
    let sdl = sdl();
    for field in [
        "packages(offset: Int!, limit: Int!, query: String!, sortBy: FileSortBy!): [Package!]!",
        "packageStatuses(ids: [ID!]!): [PackageStatus!]!",
        "packageCount(query: String!): Int!",
        "notifications(offset: Int!, limit: Int!, query: String!): [Notification!]!",
        "notificationCount(query: String!): Int!",
        "uninstallPackages(ids: [ID!]!): Boolean!",
        "installPackage(path: String!): PackageInstallPending!",
        "deleteNotifications(ids: [ID!]!): ActionResult!",
        "replyNotification(id: ID!, actionIndex: Int!, text: String!): Boolean!",
        "clipboardItems(offset: Int!, limit: Int!, query: String!): [ClipboardItem!]!",
        "clipboardItemCount(query: String!): Int!",
        "setClipboard(text: String!): Boolean!",
        "deleteClipboardItems(query: String!): ActionResult!",
        "contacts(offset: Int!, limit: Int!, query: String!): [Contact!]!",
        "contactCount(query: String!): Int!",
        "contactSources: [ContactSource!]!",
        "contactGroups: [ContactGroup!]!",
        "createContact(input: ContactInput!): Contact!",
        "updateContact(id: ID!, input: ContactInput!): Contact!",
        "deleteContacts(query: String!): ActionResult!",
        "createContactGroup(name: String!, accountName: String!, accountType: String!): ContactGroup!",
        "updateContactGroup(id: ID!, name: String!): ContactGroup!",
        "deleteContactGroup(id: ID!): Boolean!",
    ] {
        assert!(sdl.contains(field), "missing {field}");
    }
    assert!(
        sdl.contains("replyActions: [String!]!"),
        "replyActions renamed"
    );
    // The contract's Tag carries no numeric kind — that is the `tags(type:)`
    // filter — so it must not be the app's wider content_types::Tag.
    assert!(
        sdl.contains("type Tag {\n\tid: ID!\n\tname: String!\n\tcount: Int!\n}"),
        "Tag shape drifted from the contract:\n{}",
        sdl.split("type Tag {")
            .nth(1)
            .map(|rest| format!("type Tag {{{rest}"))
            .unwrap_or_default()
    );
    for field in [
        "serialNumber: String!",
        "validFrom: Instant!",
        "validTo: Instant!",
    ] {
        assert!(sdl.contains(field), "certificate field renamed: {field}");
    }
}
