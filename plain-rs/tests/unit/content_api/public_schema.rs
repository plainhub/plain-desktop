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

/// Rendering the SDL cannot tell a working resolver from one whose
/// `ctx.data::<T>()` was never registered — both print the same field, and
/// both satisfy the snapshot and contract assertions above. The pomodoro and
/// feed-sync roots shipped that way: present in every snapshot, broken for
/// every client (`Data Arc<...> does not exist`), because their services were
/// registered as `Arc<Arc<_>>`.
#[tokio::test]
async fn public_schema_pomodoro_roots_execute() {
    let db = Arc::new(Db::open(std::path::Path::new(":memory:")).unwrap());
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&dir.path().join("system_prefs.json")).unwrap());
    let (events, _) = tokio::sync::broadcast::channel(16);
    let schema = build(
        Arc::new(crate::content_api::host::Host::default()),
        events,
        prefs,
        db,
        dir.path().to_path_buf(),
    );

    for query in [
        "{ pomodoroToday { date completedCount } }",
        "{ pomodoroSettings { workDurationMin } }",
        "{ feedSyncStates { feedId status } }",
        "{ feeds { id name } }",
    ] {
        let response = schema.execute(query).await;
        assert!(
            response.errors.is_empty(),
            "{query} -> {:?}",
            response.errors
        );
    }
}

/// Scalars and enums a probe can ask for without pulling in a whole object
/// graph; anything else is skipped as a selection leaf.
const PROBE_LEAVES: [&str; 12] = [
    "Int",
    "String",
    "Boolean",
    "Float",
    "ID",
    "Long",
    "Instant",
    "JSON",
    "DateTime",
    "FileSortBy",
    "MediaDataType",
    "DataType",
];

/// Parses the committed SDL into `(type name, field, field type)` triples.
/// Deliberately a dumb line reader: the SDL is already frozen against the
/// built schema by the test above, so this only has to be consistent with it.
///
/// Two things this has to get right, both of which silently turn the walk
/// into a no-op rather than into an error:
///   * every braced block counts, not just `type` — otherwise an `enum`'s
///     values land on whichever type happened to precede it;
///   * a field's type is whatever follows the *argument list's* closing
///     paren. Splitting on the first colon reads `offset: Int!` as the type
///     and leaves every root that takes arguments unprobeable;
///   * a `"""` description can span lines and its continuation lines look
///     exactly like fields, colon and all.
fn sdl_fields(sdl: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut owner = String::new();
    let mut depth = 0usize;
    let mut in_description = false;
    for raw in sdl.lines() {
        let line = raw.trim();
        if in_description {
            if line.ends_with("\"\"\"") {
                in_description = false;
            }
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let keyword = line.split_whitespace().next().filter(|k| {
            matches!(
                *k,
                "type" | "interface" | "input" | "enum" | "scalar" | "union"
            )
        });
        if let Some(keyword) = keyword {
            if line.ends_with('{') {
                depth += 1;
                owner = if matches!(keyword, "type" | "interface") {
                    line.split_whitespace()
                        .nth(1)
                        .unwrap_or_default()
                        .to_string()
                } else {
                    String::new()
                };
                continue;
            }
        }
        if line.starts_with('}') {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                owner.clear();
            }
            continue;
        }
        if line.starts_with("\"\"\"") {
            // A one-line description opens and closes on the same line; only an
            // unterminated one puts the parser into description mode.
            // A bare `"""` on its own line opens a block that ends on some
            // later line, so length matters as much as the suffix.
            in_description = !(line.ends_with("\"\"\"") && line.len() > 3);
            continue;
        }
        if owner.is_empty() {
            continue;
        }
        let (field, ty) = match line.find('(') {
            Some(open) => {
                let close = line[open..]
                    .find(')')
                    .map(|i| i + open)
                    .unwrap_or_else(|| line.len() - 1);
                (
                    line[..open].trim().to_string(),
                    line[close + 1..].trim_start_matches(':').trim().to_string(),
                )
            }
            None => match line.split_once(':') {
                Some((n, t)) => (n.trim().to_string(), t.trim().to_string()),
                None => continue,
            },
        };
        if !field.is_empty() && !field.contains(' ') {
            out.push((owner.clone(), field, ty));
        }
    }
    out
}

/// Enum names in the SDL. An enum is a leaf for query purposes — it needs no
/// selection set — so it belongs with the scalars, not with the objects.
fn sdl_enums(sdl: &str) -> Vec<String> {
    sdl.lines()
        .map(str::trim)
        .filter(|l| l.starts_with("enum ") && l.ends_with('{'))
        .filter_map(|l| l.split_whitespace().nth(1).map(str::to_string))
        .collect()
}

/// Strip list and non-null wrappers down to the named type. Order matters:
/// `[AppFile!]!` does not yield to trim_start/trim_end pairs, because the
/// trailing `!` sits outside the closing bracket.
fn base_type(ty: &str) -> String {
    ty.trim()
        .chars()
        .filter(|c| !matches!(c, '[' | ']' | '!'))
        .collect()
}

#[tokio::test]
async fn no_root_field_fails_for_unregistered_schema_data() {
    let sdl = sdl();
    let fields = sdl_fields(&sdl);
    let enums = sdl_enums(&sdl);
    let is_leaf = |ty: &str| PROBE_LEAVES.contains(&ty) || enums.iter().any(|e| e == ty);
    let leaves = |ty: &str| -> Vec<String> {
        let ty = base_type(ty);
        fields
            .iter()
            .filter(|(owner, _, _)| owner.as_str() == ty)
            .filter(|(_, _, field_ty)| is_leaf(&base_type(field_ty)))
            .map(|(_, name, _)| name.clone())
            .take(3)
            .collect()
    };

    let roots: Vec<(String, String)> = fields
        .iter()
        .filter(|(owner, _, _)| owner == "Query")
        .map(|(_, name, ty)| (name.clone(), ty.clone()))
        .collect();

    let db = Arc::new(Db::open(std::path::Path::new(":memory:")).unwrap());
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(Prefs::load(&dir.path().join("system_prefs.json")).unwrap());
    let (events, _) = tokio::sync::broadcast::channel(16);
    let schema = build(
        Arc::new(crate::content_api::host::Host::default()),
        events,
        prefs,
        db,
        dir.path().to_path_buf(),
    );

    let mut unregistered = Vec::new();
    let mut unwalkable = Vec::new();
    for (name, ty) in &roots {
        let base = base_type(ty);
        let selection = leaves(ty);
        // An object type queried without a selection set fails validation before
        // any resolver runs — the probe would look "covered" while testing
        // nothing. Skipping those silently is exactly how feedSyncStates went
        // unchecked here, so an unwalkable root is a failure, not a gap.
        if selection.is_empty() && !is_leaf(&base) {
            unwalkable.push(format!("{name}: {base} has no probeable leaf field"));
            continue;
        }
        let query = if selection.is_empty() {
            format!("{{ {name} }}")
        } else {
            format!("{{ {name} {{ {} }} }}", selection.join(" "))
        };
        let response = schema.execute(query.as_str()).await;
        for error in response.errors {
            if error.message.contains("does not exist") {
                unregistered.push(format!("{name}: {}", error.message));
            }
        }
    }

    assert!(
        unwalkable.is_empty(),
        "roots this probe could not reach at all: {unwalkable:#?}"
    );
    assert!(roots.len() > 50, "only walked {} roots", roots.len());
    assert!(
        unregistered.is_empty(),
        "roots that resolve nothing because their schema data was never \
         registered: {unregistered:#?}"
    );
}
