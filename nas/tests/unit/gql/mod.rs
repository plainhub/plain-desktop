//! Unit tests for `src/gql/mod.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

/// Arc<Prefs> backed by a fresh prefs.json under `dir` (test fixture).
fn prefs(dir: &impl AsRef<std::path::Path>) -> std::sync::Arc<crate::prefs::Prefs> {
    std::sync::Arc::new(crate::prefs::Prefs::load(&dir.as_ref().join("prefs.json")).unwrap())
}

/// Serializes SDL-file access between `print_schema` (writer) and the
/// contract tests (reader) under `cargo test` parallelism. Poison is
/// tolerated on purpose: when the snapshot test fails (stale SDL), the
/// writer must still be able to regenerate the file on this very run.
static SDL_FILE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn sdl_lock() -> std::sync::MutexGuard<'static, ()> {
    SDL_FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Build the live schema SDL (fixture shared by `print_schema` and the
/// contract tests). The temp dir is leaked so the Db handle outlives the
/// schema.
fn test_sdl() -> String {
    let dir = tempfile::tempdir().expect("temp dir");
    let data_dir = dir.path().to_path_buf();
    let db = std::sync::Arc::new(crate::db::Db::open(dir.path()).expect("temp db opens"));
    std::mem::forget(dir); // the db handle must outlive the test
    let config = std::sync::Arc::new(crate::config::Config::parse(
        "[server]\nhttp_port = 8080\nhttps_port = 8443\n",
    ));
    build_schema(
        db,
        prefs(&data_dir),
        config,
        data_dir.clone(),
        crate::chat::test_state(&data_dir),
    )
    .sdl()
}

/// Dump the SDL to `apitest/schema.graphqls` so the API surface can be
/// inspected and diffed against plain-app — mirrors plain-app's
/// `PrintSchemaTest` which writes `shared/apitest/schema.graphqls`.
#[test]
fn print_schema() {
    // Hold the lock for the whole test: the write below must not race the
    // contract tests' snapshot read.
    let _g = sdl_lock();
    let sdl = test_sdl();
    assert!(sdl.contains("type Query"));

    // Custom scalars (plain-app contract).
    for field in ["scalar Long", "scalar Instant"] {
        assert!(sdl.contains(field), "SDL missing scalar `{field}`");
    }

    // Developer pages (plain-app contract) — the exact surface the web
    // UI's /developer/* views select.
    for field in [
        "appLogs(offset: Int!, limit: Int!, query: String!): [String!]",
        "appLogPath: String!",
        "dbPath: String!",
        "dataStorePath: String!",
        "dataStoreEntries: [KeyValuePair!]",
        "dbTables: [String!]",
        "dbTableRowCount(table: String!): Long!",
        "dbTableRows(table: String!, offset: Int!, limit: Int!): [String!]",
        "dbTableInfo(table: String!): DbTableInfo",
        "dbTableColumns(table: String!): [DbTableColumn!]",
        "deviceStatus: DeviceStatus",
    ] {
        assert!(sdl.contains(field), "SDL missing query `{field}`");
    }
    for field in [
        "clearAppLogs: Boolean!",
        "deleteDataStoreEntry(key: String!): Boolean!",
        "deleteDbTableRows(table: String!, ids: [String!]!): Boolean!",
    ] {
        assert!(sdl.contains(field), "SDL missing mutation `{field}`");
    }
    // plain-app DeviceInfo contract fields (web deviceInfoFragment). 64-bit
    // widths go through the Long scalar.
    for field in [
        "name: String!",
        "platform: DevicePlatform!",
        "osName: String!",
        "appBuildNumber: String!",
        "cpuArch: String!",
        "cpuModel: String",
        "totalMemory: Long!",
        "totalStorage: Long!",
        "android: AndroidExtras",
    ] {
        assert!(
            sdl.contains(field),
            "SDL missing DeviceInfo field `{field}`"
        );
    }
    // plain-app audioPlayback contract (web audioQueueGQL root field).
    for field in ["audioPlayback: AudioPlayback!", "type AudioPlayback"] {
        assert!(sdl.contains(field), "SDL missing audioPlayback `{field}`");
    }
    // Round-2 alignment: nullable currentPath (null = idle) plus the
    // transport fields; exact block shape, field-for-field with the phone.
    assert!(
        sdl.contains(
            "type AudioPlayback {\n\tcurrentPath: String\n\tmode: MediaPlayMode!\n\tisPlaying: Boolean!\n\tpositionMs: Long!\n}"
        ),
        "AudioPlayback must match the plain-app field set"
    );
    // density is a Float; BookmarkGroup gains a live itemCount. The
    // ChatItem.data union (ChatFiles/ChatImages/ChatText) is gone — file
    // ids are client-derived from `content` (2026-09-27).
    for field in [
        "type DisplayInfo {\n\twidth: Int!\n\theight: Int!\n\tdensity: Float!\n}",
        "type BookmarkGroup {\n\tid: ID!\n\tname: String!\n\tcollapsed: Boolean!\n\tsortOrder: Int!\n\titemCount: Int!\n\tcreatedAt: Instant!\n\tupdatedAt: Instant!\n}",
    ] {
        assert!(sdl.contains(field), "SDL missing round-2 block `{field}`");
    }
    assert!(
        !sdl.contains("currentPath: String!"),
        "currentPath must be nullable"
    );
    // plain-app App contract: capability declaration replaced osVersion.
    for field in [
        "capabilities: [Capability!]!",
        "enum Capability",
        "MEDIA_TRASH",
        "MIRROR_AUDIO",
        "DOC_PREVIEW",
    ] {
        assert!(
            sdl.contains(field),
            "SDL missing App feature field `{field}`"
        );
    }
    // plain-app App contract: enum-typed deviceType/channel/permissions.
    for field in [
        "deviceType: DeviceType!",
        "buildChannel: AppChannelType!",
        "permissions: [Permission!]!",
        "enum DeviceType",
        "enum AppChannelType",
        "enum Permission",
        "GITHUB",
        "WRITE_EXTERNAL_STORAGE",
        "CLIPBOARD",
    ] {
        assert!(sdl.contains(field), "SDL missing App contract `{field}`");
    }
    assert!(
        !sdl.contains("osVersion: String!\n\tchannel"),
        "App.osVersion must be gone"
    );
    // scanProgress is a top-level query, not an App field; doc preview is a
    // declared feature, not a boolean; appDir serves the data dir.
    assert_eq!(
        sdl.matches("scanProgress: ScanProgress!").count(),
        1,
        "scanProgress must exist only as a top-level query"
    );
    assert!(!sdl.contains("docPreviewAvailable"));
    assert!(!sdl.contains("dataDir"));
    // favoriteFolders is a top-level query too (plain-app App has neither).
    assert_eq!(
        sdl.matches("favoriteFolders: [FavoriteFolder!]").count(),
        1,
        "favoriteFolders must exist only as a top-level query"
    );
    // plain-app DeviceStatus contract fields (web deviceStatusFragment).
    for field in [
        "uptimeSec: Long!",
        "batteryLevel: Int",
        "charging: Boolean!",
        "temperatures: [Temperature!]!",
        "cpuUsage: Float!",
        "memoryAvailable: Long",
        "storageAvailable: Long!",
    ] {
        assert!(
            sdl.contains(field),
            "SDL missing DeviceStatus field `{field}`"
        );
    }
    // Removed surface — no compat layer (one-shot schema switch).
    for gone in [
        "battery: Battery",
        "DesktopDeviceInfo",
        "enum BatteryHealth",
        "enum BatteryStatus",
        "enum BatteryPlugged",
        "buildHost",
        "buildUser",
    ] {
        assert!(!sdl.contains(gone), "SDL must not contain `{gone}` anymore");
    }

    // plain-app audio playback surface (AudioGraphQL) — queries.
    for field in [
        "audioQueueItems(offset: Int!, limit: Int!, query: String!): [AudioItem!]",
        "audioQueueItemCount: Int!",
        "audioLyrics(path: String!): String",
        "audioPlaylists: [AudioPlaylist!]",
        "audioPlaylistItems(id: ID!, offset: Int!, limit: Int!, query: String!): [AudioItem!]",
        "audioPlaylistItemCount(id: ID!): Int!",
        "audioPlayHistory(offset: Int!, limit: Int!, query: String!): [AudioPlayHistory!]",
    ] {
        assert!(sdl.contains(field), "SDL missing audio query `{field}`");
    }
    for field in [
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
    ] {
        assert!(sdl.contains(field), "SDL missing audio mutation `{field}`");
    }
    // Play mode moved from App to audioPlayback (plain-app contract) and
    // the NAS-only whole-queue fields are gone.
    assert!(!sdl.contains("audioMode: MediaPlayMode!"));
    assert!(!sdl.contains("audioCurrent"));
    assert!(!sdl.contains("AudioPlaylistPage"));
    // Media gain the plain-app favorite flag and 64-bit Long widths.
    for field in [
        "isFavorite: Boolean!",
        "durationMs: Long!",
        "size: Long!",
        "topItemPaths: [String!]!",
        "takenAt: Instant",
        "createdAt: Instant!",
        "updatedAt: Instant!",
        "bucketId: ID!",
    ] {
        assert!(sdl.contains(field), "SDL missing media field `{field}`");
    }
    // Media list args are all required (API_SPEC §3) — sortBy included.
    for field in [
        "audios(offset: Int!, limit: Int!, query: String!, sortBy: FileSortBy!): [Audio!]",
        "images(offset: Int!, limit: Int!, query: String!, sortBy: FileSortBy!): [Image!]",
        "videos(offset: Int!, limit: Int!, query: String!, sortBy: FileSortBy!): [Video!]",
        "files(root: String!, offset: Int!, limit: Int!, query: String!, sortBy: FileSortBy!): [File!]",
        "chatItems(target: String!, offset: Int!, limit: Int!, query: String!): [ChatItem!]",
        "uploadedChunks(fileId: String!): [String!]",
        "fileInfo(path: String!, fileName: String, includeDirSize: Boolean): FileInfo!",
        "tagRelations(type: DataType!, keys: [String!]!): [TagRelation!]!",
        "mediaBuckets(type: MediaDataType!): [MediaBucket!]",
    ] {
        assert!(sdl.contains(field), "SDL missing query signature `{field}`");
    }
    // Bulk mutations return the shared ActionResult; single ops stay
    // Boolean (phone contract).
    for field in [
        "deleteFiles(paths: [String!]!): ActionResult!",
        "deleteBookmarks(ids: [ID!]!): ActionResult!",
        "deleteMediaItems(type: MediaDataType!, query: String!): ActionResult!",
        "trashMediaItems(type: MediaDataType!, query: String!): ActionResult!",
        "restoreMediaItems(type: MediaDataType!, query: String!): ActionResult!",
        "moveMediaItems(type: MediaDataType!, query: String!, destDir: String!): ActionResult!",
        "deleteChatItems(query: String!): ActionResult!",
        "deleteTag(id: ID!): Boolean!",
        "mergeChunks(fileId: String!, totalChunks: Int!, path: String!, replace: Boolean!, totalSize: Long!): MergeTask!",
        "addFavoriteFolder(rootPath: String!, fullPath: String!): [FavoriteFolder!]",
        "removeFavoriteFolder(fullPath: String!): [FavoriteFolder!]",
        "setFavoriteFolderAlias(fullPath: String!, alias: String!): [FavoriteFolder!]",
    ] {
        assert!(
            sdl.contains(field),
            "SDL missing mutation signature `{field}`"
        );
    }
    assert!(
        !sdl.contains("MediaActionResult"),
        "the old type/query echo result type must be gone"
    );
    // The MediaItem interface is a known gap (needs a referencing query).
    assert!(!sdl.contains("interface MediaItem"));
    // The typed-filter migration was rejected: every shared list/count/batch
    // op keeps the DSL `query: String!` parameter and no filter InputObject
    // may appear anywhere on the surface.
    for gone in [
        "input TextFilter",
        "input FileFilter",
        "input ChatItemFilter",
        "input MediaItemFilter",
        "input ItemFilter",
        "input LongCompare",
        "input InstantCompare",
        "enum CallType",
        "filter: ",
    ] {
        assert!(!sdl.contains(gone), "SDL must not contain `{gone}`");
    }
    for kept in [
        "audios(offset: Int!, limit: Int!, query: String!, sortBy: FileSortBy!): [Audio!]",
        "audioCount(query: String!): Int!",
        "fileCount(root: String!, query: String!): Int!",
        "addAudiosToQueue(query: String!): Boolean!",
        "deleteChatItems(query: String!): ActionResult!",
        "addToTags(type: DataType!, tagIds: [ID!]!, query: String!): Boolean!",
    ] {
        assert!(sdl.contains(kept), "DSL `query` must stay on `{kept}`");
    }

    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/apitest");
    std::fs::create_dir_all(dir).expect("mkdir apitest");
    std::fs::write(format!("{dir}/schema.graphqls"), &sdl).expect("write schema.graphqls");
}

mod contract;
