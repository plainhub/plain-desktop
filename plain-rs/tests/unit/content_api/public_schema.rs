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
    let (events, _) = tokio::sync::broadcast::channel(16);
    build(
        Arc::new(crate::content_api::host::Host::default()),
        events,
        prefs,
        db,
        dir.path().to_path_buf(),
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
        "sms(offset: Int!, limit: Int!, query: String!): [Sms!]!",
        "smsCount(query: String!): Int!",
        "smsBoxCounts: SmsCounts!",
        "smsConversations(offset: Int!, limit: Int!, query: String!): [SmsConversation!]!",
        "smsConversationCount(query: String!): Int!",
        "archivedSmsConversations(offset: Int!, limit: Int!, query: String!): [SmsConversation!]!",
        "archiveSmsConversation(id: ID!): Boolean!",
        "unarchiveSmsConversation(id: ID!): Boolean!",
        "trashSms(query: String!): ActionResult!",
        "restoreSms(query: String!): ActionResult!",
        "deleteSms(query: String!): ActionResult!",
        "sendSms(number: String!, body: String!, subscriptionId: Int!, requestId: String): Boolean!",
        "sendMms(number: String!, body: String!, attachmentPaths: [String!]!, threadId: ID!): String!",
        "calls(offset: Int!, limit: Int!, query: String!): [Call!]!",
        "callCount(query: String!): Int!",
        "call(number: String!, showDialer: Boolean!): Boolean!",
        "deleteCalls(query: String!): ActionResult!",
        "isScreenMirroring: Boolean!",
        "screenMirrorVideoCodec: ScreenMirrorVideoCodec",
        "screenMirrorControlEnabled: Boolean!",
        "screenMirrorQuality: ScreenMirrorQuality!",
        "startScreenMirror(audio: Boolean!): Boolean!",
        "requestScreenMirrorAudio: Boolean!",
        "stopScreenMirror: Boolean!",
        "updateScreenMirrorQuality(mode: ScreenMirrorMode!): Boolean!",
        "requestScreenMirrorKeyFrame: Boolean!",
        "imageSearchStatus: ImageSearchStatus!",
        "enableImageSearch: Boolean!",
        "disableImageSearch: Boolean!",
        "cancelImageModelDownload: Boolean!",
        "startImageIndex(force: Boolean): Boolean!",
        "cancelImageIndex: Boolean!",
        "openAccessibilitySettings: Boolean!",
        "openWebSettings(feature: WebSettingsFeature): Boolean!",
        "appFiles(offset: Int!, limit: Int!, query: String!): [AppFile!]!",
        "appFileCount(query: String!): Int!",
        "bookmarks: [Bookmark!]!",
        "bookmarkGroups: [BookmarkGroup!]!",
        "addBookmarks(urls: [String!]!, groupId: ID!): [Bookmark!]!",
        "updateBookmark(id: ID!, input: BookmarkInput!): Bookmark!",
        "deleteBookmarks(ids: [ID!]!): ActionResult!",
        "recordBookmarkClick(id: ID!): Boolean!",
        "createBookmarkGroup(name: String!): BookmarkGroup!",
        "updateBookmarkGroup(id: ID!, name: String!, collapsed: Boolean!, sortOrder: Int!): BookmarkGroup!",
        "deleteBookmarkGroup(id: ID!): Boolean!",
        "pomodoroSettings: PomodoroSettings!",
        "pomodoroToday: PomodoroToday!",
        "startPomodoro(durationSec: Int!): Boolean!",
        "pausePomodoro: Boolean!",
        "stopPomodoro: Boolean!",
        "imageEditorProjects: [ImageEditorProjectSummary!]!",
        "imageEditorProject(id: ID!): ImageEditorProject",
        "saveImageEditorProject(id: ID!, input: ImageEditorProjectInput!): ImageEditorProject!",
        "deleteImageEditorProject(id: ID!): Boolean!",
        "broadcastImageEditorUpdate(id: ID!, update: String!): Boolean!",
        "noteCount(query: String!): Int!",
        "note(id: ID!): Note",
        "notes(offset: Int!, limit: Int!, query: String!): [Note!]!",
        "createNote(input: NoteInput!): Note!",
        "updateNote(id: ID!, input: NoteInput!): Note!",
        "saveFeedEntriesToNotes(query: String!): [ID!]!",
        "trashNotes(query: String!): ActionResult!",
        "restoreNotes(query: String!): ActionResult!",
        "deleteNotes(query: String!): ActionResult!",
        "exportNotes(query: String!): String!",
        "feedSyncStates: [FeedSyncState!]!",
        "feed(id: ID!): Feed",
        "feeds: [Feed!]!",
        "feedEntryCounts: [FeedEntryCount!]!",
        "feedEntryCount(query: String!): Int!",
        "feedEntry(id: ID!): FeedEntry",
        "feedEntries(offset: Int!, limit: Int!, query: String!): [FeedEntry!]!",
        "updateFeedUrl(id: ID!, url: String!): Feed!",
        "markFeedEntriesRead(query: String!, read: Boolean!): ActionResult!",
        "syncFeeds(id: ID): Boolean!",
        "updateFeed(id: ID!, name: String!, fetchContent: Boolean!): Feed!",
        "createFeed(url: String!, fetchContent: Boolean!): Feed!",
        "importFeeds(content: String!): Boolean!",
        "exportFeeds: String!",
        "deleteFeed(id: ID!): Boolean!",
        "syncFeedEntryContent(id: ID!): FeedEntry!",
        "deleteFeedEntries(query: String!): ActionResult!",
        "audioCount(query: String!): Int!",
        "audios(offset: Int!, limit: Int!, query: String!, sortBy: FileSortBy!): [Audio!]!",
        "audioLyrics(path: String!): String",
        "audioQueueItems(offset: Int!, limit: Int!, query: String!): [AudioItem!]!",
        "audioQueueItemCount: Int!",
        "audioPlayback: AudioPlayback!",
        "audioPlaylists: [AudioPlaylist!]!",
        "audioPlaylistItems(id: ID!, offset: Int!, limit: Int!, query: String!): [AudioItem!]!",
        "audioPlaylistItemCount(id: ID!): Int!",
        "audioPlayHistory(offset: Int!, limit: Int!, query: String!): [AudioPlayHistory!]!",
        "playAudio(path: String!): AudioItem!",
        "updateAudioPlayMode(mode: MediaPlayMode!): Boolean!",
        "clearAudioQueue: Boolean!",
        "removeAudioFromQueue(path: String!): Boolean!",
        "addAudiosToQueue(query: String!): Boolean!",
        "reorderAudioQueue(paths: [String!]!): Boolean!",
        "createAudioPlaylist(name: String!): AudioPlaylist!",
        "updateAudioPlaylist(id: ID!, name: String!): AudioPlaylist!",
        "deleteAudioPlaylist(id: ID!): Boolean!",
        "addAudioPlaylistItems(id: ID!, paths: [String!]!): Boolean!",
        "removeAudioPlaylistItem(id: ID!, path: String!): Boolean!",
        "playAudioPlaylist(id: ID!, path: String, shuffle: Boolean!): AudioItem",
        "playAllAudios(shuffle: Boolean!): AudioItem",
        "chatChannels: [ChatChannel!]!",
        "chatItems(target: String!, offset: Int!, limit: Int!, query: String!): [ChatItem!]!",
        "latestChatItems: [ChatItem!]!",
        "sendChatItem(target: String!, content: String!): [ChatItem!]!",
        "deleteChatItem(id: ID!): Boolean!",
        "deleteChatItems(query: String!): ActionResult!",
        "retryChatItem(id: ID!): ChatItem!",
        "createChatChannel(name: String!): ChatChannel!",
        "updateChatChannel(id: ID!, name: String!): ChatChannel!",
        "deleteChatChannel(id: ID!): Boolean!",
        "leaveChatChannel(id: ID!): Boolean!",
        "addChatChannelMember(id: ID!, peerId: ID!): ChatChannel!",
        "removeChatChannelMember(id: ID!, peerId: ID!): ChatChannel!",
        "acceptChatChannelInvite(id: ID!): Boolean!",
        "declineChatChannelInvite(id: ID!): Boolean!",
        "peers: [Peer!]!",
        "sims: [Sim!]!",
        "pairDevice(input: PairingDeviceInput!): Boolean!",
        "cancelPairing(deviceId: ID!): Boolean!",
        "respondToPairing(input: PairingRequestInput!, accepted: Boolean!): Boolean!",
        "deletePeer(id: ID!): Boolean!",
        "unpairPeer(id: ID!): Boolean!",
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
