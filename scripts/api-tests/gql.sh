#!/usr/bin/env bash
# 测试所有 GraphQL API 的 curl 脚本。
#
# 用法 (仓库根目录):
#   1. 启动服务:
#        RUST_LOG=info PLAIN_NAS_ALLOW_NONROOT=1 \
#          PLAIN_NAS_DATA_DIR=./nas/tmp-data \
#          cargo run -p plainnas -- run &
#   2. 跑测试:
#        ./nas/scripts/api-tests/gql.sh
#
# 所有调用走 dev 模式 (Authorization: Bearer dev)。
# 输出按"测试名 / query / data"格式打印。
# 失败 (errors) 会在 stderr 标红。

set -uo pipefail

HOST="${HOST:-127.0.0.1:8180}"
DEV_TOKEN="${DEV_TOKEN:-dev}"
PASS=0
FAIL=0

GQL="http://$HOST/graphql"
AUTH_HDR="Authorization: Bearer $DEV_TOKEN"
JSON_HDR="Content-Type: application/json"

# --- helpers -----------------------------------------------------------------

red()   { printf '\033[31m%s\033[0m' "$*"; }
green() { printf '\033[32m%s\033[0m' "$*"; }
gray()  { printf '\033[90m%s\033[0m' "$*"; }

# gql_call <label> <query> [variables-json]
# Prints:
#   PASS <label>
#   <response>   (just the .data portion, or errors if any)
# Returns 0 if no errors, 1 if errors.
gql_call() {
  local label="$1"
  local query="$2"
  local vars="${3:-}"
  local body
  if [ -n "$vars" ]; then
    body=$(printf '{"query":%s,"variables":%s}' "$(jq -Rs . <<<"$query")" "$vars")
  else
    body=$(printf '{"query":%s}' "$(jq -Rs . <<<"$query")")
  fi
  local resp
  resp=$(curl -s -X POST -H "$AUTH_HDR" -H "$JSON_HDR" "$GQL" -d "$body")
  local errs
  errs=$(echo "$resp" | jq -r '.errors // empty' 2>/dev/null)
  if [ -n "$errs" ]; then
    FAIL=$((FAIL+1))
    printf '%s %s\n' "$(red '[FAIL]')" "$label"
    echo "  query:   $query"
    echo "  errors:  $errs" | head -3
    return 1
  fi
  PASS=$((PASS+1))
  printf '%s %s\n' "$(green '[PASS]')" "$label"
  echo "$resp" | jq -c '.data' 2>/dev/null
}

section() { printf '\n%s\n' "$(gray "### $* ###")"; }

# gql_expect_fail <label> <query>
# Inverse of gql_call: PASS when the server returns errors (used for error-path tests).
gql_expect_fail() {
  local label="$1"
  local query="$2"
  local body
  body=$(printf '{"query":%s}' "$(jq -Rs . <<<"$query")")
  local resp
  resp=$(curl -s -X POST -H "$AUTH_HDR" -H "$JSON_HDR" "$GQL" -d "$body")
  local errs
  errs=$(echo "$resp" | jq -r '.errors // empty' 2>/dev/null)
  if [ -n "$errs" ]; then
    PASS=$((PASS+1))
    printf '%s %s\n' "$(green '[PASS]')" "$label (expected error)"
    return 0
  fi
  FAIL=$((FAIL+1))
  printf '%s %s\n' "$(red '[FAIL]')" "$label (expected error but got none)"
  return 1
}

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "ERROR: $1 not found in PATH" >&2
    exit 127
  fi
}

# --- preflight ---------------------------------------------------------------

need curl
need jq

# Health check.
if ! curl -fsS "http://$HOST/health" >/dev/null 2>&1; then
  echo "ERROR: $HOST/health not responding" >&2
  exit 1
fi
echo ">>> target: $GQL  (dev mode)"

# --- tests -------------------------------------------------------------------

section "App / Device / Update"
gql_call "app"   '{ app { httpPort httpsPort urlToken appDir deviceName capabilities } }'
gql_call "deviceInfo"  '{ deviceInfo { name platform manufacturer model osName osVersion cpuArch cpuModel totalMemory totalStorage } }'
gql_call "deviceStatus"  '{ deviceStatus { uptimeSec batteryLevel charging cpuUsage memoryAvailable storageAvailable temperatures { label celsius } } }'
gql_call "appUpdate"  '{ appUpdate { currentVersion hasUpdate latestVersion url } }'

section "Storage"
gql_call "mounts"   '{ mounts { id name mountPoint fsType totalBytes usedBytes freeBytes diskID remote driveType } }'
gql_call "disks"    '{ disks { id name path sizeBytes removable model } }'

section "Files"
gql_call "files"   '{ files(offset: 0, limit: 5, query: "") { path isDir size childCount } }'
gql_call "fileCount"  '{ fileCount(query: "") }'
gql_call "recentFiles"  '{ recentFiles { path isDir size } }'
gql_call "recentFilesCount"  '{ recentFilesCount }'
gql_call "pathExists"  '{ pathExists(path: "/tmp") }'
gql_call "pathKind"    '{ pathKind(path: "/tmp") }'
gql_call "pathKind missing"  '{ pathKind(path: "/no/such/path") }'
gql_call "fileInfo"  '{ fileInfo(path: "/tmp") { path updatedAt size tags { id name } } }'

section "Trash"
gql_call "trashedFileCount"  '{ trashedFileCount }'
gql_call "trashedFiles"  '{ trashedFiles(offset: 0, limit: 5, query: "", sortBy: DATE_DESC) { id type originalPath displayName } }'

section "Tags"
gql_call "tags Audio"  '{ tags(type: AUDIO) { id name count } }'

section "Tasks"
gql_call "fileTasks"  '{ fileTasks { id type status } }'

section "Favorites"
gql_call "favoriteFolders"  '{ favoriteFolders { rootPath relativePath alias } }'

section "Playlist"
gql_call "audioPlayback"   '{ audioPlayback { mode currentPath } }'
gql_call "audios"  '{ audios(offset: 0, limit: 5, query: "") { id title artist path duration size } }'
gql_call "audioCount"  '{ audioCount(query: "") }'

section "Media (stubs)"
gql_call "images"  '{ images(offset: 0, limit: 5, query: "") { id title path size } }'
gql_call "imageCount"  '{ imageCount(query: "") }'
gql_call "videos"  '{ videos(offset: 0, limit: 5, query: "") { id title path duration } }'
gql_call "videoCount"  '{ videoCount(query: "") }'
gql_call "mediaBuckets"  '{ mediaBuckets(type: AUDIO) { id name itemCount } }'
gql_call "imageSearchStatus"  '{ imageSearchStatus { status downloadProgress errorMessage modelSize modelDir isIndexing totalImages indexedImages } }'

section "Chunked upload"
gql_call "uploadedChunks"  '{ uploadedChunks(fileId: "test-fake") }'

section "Samba"
gql_call "sambaSettings"  '{ sambaSettings { enabled username hasPassword shares { name sharePath auth readOnly } serviceName serviceActive serviceEnabled } }'

section "DLNA"
gql_call "dlnaRenderers"  '{ dlnaRenderers { udn name location } }'

section "Media source dirs"
gql_call "mediaSourceDirs"  '{ mediaSourceDirs }'

section "Events / Sessions"
gql_call "sessions"  '{ sessions { clientId clientName } }'
gql_call "auditEvents"    '{ auditEvents(offset: 0, limit: 5, query: "") { id type message createdAt } }'

# --- mutations ---------------------------------------------------------------

section "Mutations: temp values"
gql_call "setTempValue"    'mutation { setTempValue(key: "test.t", value: "v") { key value } }'

section "Mutations: Files"
gql_call "createDir"   'mutation { createDir(path: "/tmp/test-api") { path isDir } }'
gql_call "writeTextFile"  'mutation { writeTextFile(path: "/tmp/test-api/note.txt", content: "hi", overwrite: false) { path size } }'
gql_call "renameFile"  'mutation { renameFile(path: "/tmp/test-api/note.txt", name: "note2.txt") }'
gql_call "copyFile"    'mutation { copyFile(src: "/tmp/test-api/note2.txt", dst: "/tmp/test-api/note3.txt", overwrite: true) }'
gql_call "moveFile"    'mutation { moveFile(src: "/tmp/test-api/note3.txt", dst: "/tmp/test-api/note4.txt", overwrite: true) }'

section "Mutations: Trash"
# trashFiles needs write access to ${MOUNT}/.nas-trash/ — fails as non-root
# when the mount root is /. We still exercise the call but don't count it.
section "Mutations: Trash (trashFiles skipped if non-root)"
if [ "$(id -u)" -ne 0 ]; then
  printf '%s %s\n' "$(gray '[SKIP]')" "trashFiles (requires root for /.nas-trash/)"
else
  gql_call "trashFiles"   'mutation { trashFiles(paths: ["/tmp/test-api"]) { affectedCount } }'
fi
gql_call "trashedFiles after trash"  '{ trashedFiles(offset: 0, limit: 5, query: "", sortBy: DATE_DESC) { id type deletedAt sizeBytes } }'

section "Mutations: Files cleanup"
gql_call "deleteFiles" 'mutation { deleteFiles(paths: ["/tmp/test-api/note2.txt", "/tmp/test-api/note4.txt"]) }'

section "Mutations: Tags"
# Capture the auto-generated tag id from createTag response
CREATE_TAG_RESP=$(curl -s -X POST -H "$AUTH_HDR" -H "$JSON_HDR" "$GQL" \
  -d '{"query":"mutation { createTag(type: AUDIO, name: \"test-tag\") { id name count } }"}')
TAG_ID=$(echo "$CREATE_TAG_RESP" | jq -r '.data.createTag.id')
if [ -n "$TAG_ID" ] && [ "$TAG_ID" != "null" ]; then
  PASS=$((PASS+1))
  printf '%s %s\n' "$(green '[PASS]')" "createTag (id=$TAG_ID)"
else
  FAIL=$((FAIL+1))
  printf '%s %s\n' "$(red '[FAIL]')" "createTag"
  echo "  errors:  $CREATE_TAG_RESP" | head -3
fi
gql_call "updateTag"   "mutation { updateTag(id: \"$TAG_ID\", name: \"test-tag2\") { id name } }"
gql_call "addToTags"   'mutation { addToTags(type: AUDIO, tagIds: ["test-tag2"], query: "/tmp/fake.mp3") }'
gql_call "removeFromTags"  'mutation { removeFromTags(type: AUDIO, tagIds: ["test-tag2"], query: "/tmp/fake.mp3") }'
gql_call "deleteTag"   "mutation { deleteTag(id: \"$TAG_ID\") }"

section "Mutations: Favorites"
gql_call "addFavoriteFolder"    'mutation { addFavoriteFolder(rootPath: "/tmp", relativePath: "test-api") { rootPath relativePath } }'
gql_call "setFavoriteFolderAlias"  'mutation { setFavoriteFolderAlias(rootPath: "/tmp", relativePath: "test-api", alias: "Test") }'
gql_call "removeFavoriteFolder" 'mutation { removeFavoriteFolder(rootPath: "/tmp", relativePath: "test-api") { rootPath relativePath } }'

section "Mutations: Sessions"
gql_call "revokeSession"  'mutation { revokeSession(clientId: "nonexistent-client") }'

section "Mutations: Playlist"
gql_call "addPlaylistAudios"   'mutation { addPlaylistAudios(query: "") }'
gql_call "reorderPlaylistAudios"  'mutation { reorderPlaylistAudios(paths: []) }'
gql_call "updateAudioPlayMode" 'mutation { updateAudioPlayMode(mode: REPEAT) }'
gql_call "deletePlaylistAudio"  'mutation { deletePlaylistAudio(path: "/tmp/fake.mp3") }'
gql_call "clearAudioPlaylist"  'mutation { clearAudioPlaylist }'

section "Mutations: Samba"
gql_call "setSambaSettings"    'mutation { setSambaSettings(input: { enabled: false, shares: [] }) }'
gql_call "setSambaUserPassword" 'mutation { setSambaUserPassword(password: "test123") }'

section "Mutations: Device"
# setDeviceName requires hostnamectl + root; expect FAIL in non-root environments
if [ "$(id -u)" -ne 0 ]; then
  printf '%s %s\n' "$(gray '[SKIP]')" "setDeviceName (requires root)"
else
  gql_call "setDeviceName"  'mutation { setDeviceName(name: "test-nas") }'
fi

section "Mutations: Trash cleanup"
# restoreFiles + deleteTrashedFile with fake paths exercise error handling.
gql_expect_fail "restoreFiles"  'mutation { restoreFiles(paths: ["/tmp/fake-trash"]) }'
gql_expect_fail "deleteTrashedFile"   'mutation { deleteTrashedFile(path: "/tmp/fake-trash") }'

section "Mutations: Storage"
gql_call "setMountAlias"        'mutation { setMountAlias(id: "fake", alias: "x") }'
gql_call "setMediaSourceDirs"   'mutation { setMediaSourceDirs(dirs: []) }'

section "Mutations: Tags (extended)"
CREATE_TAG2_RESP=$(curl -s -X POST -H "$AUTH_HDR" -H "$JSON_HDR" "$GQL" \
  -d '{"query":"mutation { createTag(type: AUDIO, name: \"rel-tag\") { id name count } }"}')
TAG2_ID=$(echo "$CREATE_TAG2_RESP" | jq -r '.data.createTag.id')
if [ -n "$TAG2_ID" ] && [ "$TAG2_ID" != "null" ]; then
  PASS=$((PASS+1))
  printf '%s %s\n' "$(green '[PASS]')" "createTag2 (id=$TAG2_ID)"
else
  FAIL=$((FAIL+1))
  printf '%s %s\n' "$(red '[FAIL]')" "createTag2"
  echo "  errors:  $CREATE_TAG2_RESP" | head -3
fi
gql_call "updateTagRelations"  "mutation { updateTagRelations(type: AUDIO, item: { key: \"/tmp/fake.mp3\", title: \"fake\", size: 0 }, addTagIds: [\"$TAG2_ID\"], removeTagIds: []) }"
gql_call "removeFromTags2"  "mutation { removeFromTags(type: AUDIO, tagIds: [\"$TAG2_ID\"], query: \"/tmp/fake.mp3\") }"
gql_call "deleteTag2"   "mutation { deleteTag(id: \"$TAG2_ID\") }"

section "Mutations: Media items (stubs)"
gql_call "trashMediaItems"    'mutation { trashMediaItems(type: AUDIO, query: "") { type query } }'
gql_call "restoreMediaItems"  'mutation { restoreMediaItems(type: AUDIO, query: "") { type query } }'
gql_call "deleteMediaItems"   'mutation { deleteMediaItems(type: AUDIO, query: "") { type query } }'

section "Mutations: Tasks"
gql_call "createCopyTask"  'mutation { createCopyTask(ops: [{ src: "/tmp/test-api", dst: "/tmp/test-api-copy", overwrite: true }]) { id type status } }'
gql_call "createMoveTask"  'mutation { createMoveTask(ops: [{ src: "/tmp/test-api-copy", dst: "/tmp/test-api-move", overwrite: true }]) { id type status } }'

section "Mutations: Scan"
gql_call "startMediaScan"   'mutation { startMediaScan(root: "/tmp") }'
gql_call "pauseMediaScan"   'mutation { pauseMediaScan }'
gql_call "resumeMediaScan"  'mutation { resumeMediaScan }'
gql_call "stopMediaScan"    'mutation { stopMediaScan }'

section "Mutations: Scan (只触发 rebuildMediaIndex，不等结果)"
gql_call "rebuildMediaIndex"  'mutation { rebuildMediaIndex(root: "/tmp") }'
# 等 2s 让 walk 启动，至少能看到状态切换
sleep 2
gql_call "scanProgress after rebuild"  '{ scanProgress { indexed pending total state } }'

section "Mutations: Bookmarks (full roundtrip)"
# Capture ids from create responses, then update/click/delete.
CREATE_BM_RESP=$(curl -s -X POST -H "$AUTH_HDR" -H "$JSON_HDR" "$GQL" \
  -d '{"query":"mutation { addBookmarks(urls: [\"https://example.com/a\", \" \"], groupId: \"\") { id url title pinned clickCount faviconPath } }"}')
BM_ID=$(echo "$CREATE_BM_RESP" | jq -r '.data.addBookmarks[0].id')
if [ -n "$BM_ID" ] && [ "$BM_ID" != "null" ]; then
  PASS=$((PASS+1))
  printf '%s %s\n' "$(green '[PASS]')" "addBookmarks (id=$BM_ID, empty URL skipped: $(echo "$CREATE_BM_RESP" | jq '.data.addBookmarks | length'))"
else
  FAIL=$((FAIL+1))
  printf '%s %s\n' "$(red '[FAIL]')" "addBookmarks"
  echo "  resp: $CREATE_BM_RESP" | head -3
fi
CREATE_BG_RESP=$(curl -s -X POST -H "$AUTH_HDR" -H "$JSON_HDR" "$GQL" \
  -d '{"query":"mutation { createBookmarkGroup(name: \"test-group\") { id name collapsed sortOrder } }"}')
BG_ID=$(echo "$CREATE_BG_RESP" | jq -r '.data.createBookmarkGroup.id')
if [ -n "$BG_ID" ] && [ "$BG_ID" != "null" ]; then
  PASS=$((PASS+1))
  printf '%s %s\n' "$(green '[PASS]')" "createBookmarkGroup (id=$BG_ID)"
else
  FAIL=$((FAIL+1))
  printf '%s %s\n' "$(red '[FAIL]')" "createBookmarkGroup"
  echo "  resp: $CREATE_BG_RESP" | head -3
fi
gql_call "bookmarks"          '{ bookmarks { id url title groupId pinned clickCount lastClickedAt sortOrder createdAt updatedAt } }'
gql_call "bookmarkGroups"     '{ bookmarkGroups { id name collapsed sortOrder createdAt updatedAt } }'
gql_call "updateBookmark"     "mutation { updateBookmark(id: \"$BM_ID\", input: { url: \"https://example.com/b\", title: \"B\", groupId: \"$BG_ID\", pinned: true, sortOrder: 3 }) { id url title groupId pinned sortOrder } }"
gql_call "updateBookmarkGroup" "mutation { updateBookmarkGroup(id: \"$BG_ID\", name: \"test-group2\", collapsed: true, sortOrder: 2) { id name collapsed sortOrder } }"
gql_call "recordBookmarkClick" "mutation { recordBookmarkClick(id: \"$BM_ID\") }"
# deleteBookmarkGroup must ungroup members (groupId -> ""), matching plain-app
gql_call "deleteBookmarkGroup" "mutation { deleteBookmarkGroup(id: \"$BG_ID\") }"
gql_call "bookmark ungrouped after group delete" "{ bookmarks { id groupId } }"
gql_call "deleteBookmarks"    "mutation { deleteBookmarks(ids: [\"$BM_ID\"]) }"

section "Audio playback queue (plain-app AudioGraphQL)"
gql_call "playAudio (seed queue)"   'mutation { playAudio(path: "/tmp/fake.mp3") { title artist path duration } }'
gql_call "audioQueueItems"          '{ audioQueueItems(offset: 0, limit: 10) { title artist path duration } }'
gql_call "audioQueueItemCount"      '{ audioQueueItemCount }'
gql_call "audioQueueItems offset"   '{ audioQueueItems(offset: 1, limit: 10) { path } }'
gql_call "audioLyrics (no tags)"    '{ audioLyrics(path: "/tmp/fake.mp3") }'
gql_call "audioPlayHistory"         '{ audioPlayHistory(limit: 10, offset: 0) { path playCount } }'
AP_RESP=$(curl -s -X POST -H "$AUTH_HDR" -H "$JSON_HDR" "$GQL" \
  -d '{"query":"mutation { createAudioPlaylist(name: \"smoke\") { id name itemCount } }"}')
if [ -n "$(echo "$AP_RESP" | jq -r '.errors // empty' 2>/dev/null)" ]; then
  FAIL=$((FAIL+1)); printf '%s %s\n' "$(red '[FAIL]')" "createAudioPlaylist"
else
  PASS=$((PASS+1)); printf '%s %s\n' "$(green '[PASS]')" "createAudioPlaylist ($(echo "$AP_RESP" | jq -c '.data.createAudioPlaylist'))"
fi
AP_ID=$(echo "$AP_RESP" | jq -r '.data.createAudioPlaylist.id')
gql_call "addAudioPlaylistItems"    "mutation { addAudioPlaylistItems(id: \"$AP_ID\", paths: [\"/tmp/fake.mp3\"]) }"
gql_call "audioPlaylists"           '{ audioPlaylists { id name itemCount } }'
gql_call "audioPlaylistItems"       "{ audioPlaylistItems(id: \"$AP_ID\", offset: 0, limit: 10) { path } }"
gql_call "audioPlaylistItemCount"   "{ audioPlaylistItemCount(id: \"$AP_ID\") }"
gql_call "playAudioPlaylist (empty start ok)" "mutation { playAudioPlaylist(id: \"$AP_ID\", shuffle: false) }"
gql_call "playAllAudios"            'mutation { playAllAudios(shuffle: false) }'
gql_call "removeAudioPlaylistItem"  'mutation { removeAudioPlaylistItem(id: "'$AP_ID'", path: "/tmp/fake.mp3") }' 
gql_call "updateAudioPlaylist"      "mutation { updateAudioPlaylist(id: \"$AP_ID\", name: \"smoke2\") { id name } }"
gql_call "deleteAudioPlaylist"      "mutation { deleteAudioPlaylist(id: \"$AP_ID\") }"
gql_call "deletePlaylistAudio"      'mutation { deletePlaylistAudio(path: "/tmp/fake.mp3") }'

section "Developer pages (logs / datastore / database / device-info)"
gql_call "appLogPath"     '{ appLogPath }'
gql_call "appLogs"        '{ appLogs(offset: 0, limit: 10) }'
gql_call "clearAppLogs"   'mutation { clearAppLogs }'
gql_call "dbPath"         '{ dbPath }'
gql_call "dataStorePath"  '{ dataStorePath }'
gql_call "dbTables"       '{ dbTables }'
gql_call "dbTableInfo"    '{ dbTableInfo(table: "tag") { idKey } }'
gql_call "dbTableRowCount" '{ dbTableRowCount(table: "tag") }'
gql_call "dbTableRows tag" '{ dbTableRows(table: "tag", offset: 0, limit: 50) }'
gql_expect_fail "deleteDbTableRows rejects cross-table id" 'mutation { deleteDbTableRows(table: "tag", ids: ["session:api-test-x"]) }'
gql_expect_fail "setKeyValue removed (Go legacy)" 'mutation { setKeyValue(key: "k", value: "v") }'
gql_expect_fail "dbTableInfo rejects prefs tables" '{ dbTableInfo(table: "system") { idKey } }'

section "Mutations: Logout (last — invalidates session)"
gql_call "logout"  'mutation { logout }'

# --- cleanup -----------------------------------------------------------------

section "Cleanup"
# Remove the test directories we created
rm -rf /tmp/test-api /tmp/test-api-copy /tmp/test-api-move 2>/dev/null || true

# --- summary -----------------------------------------------------------------

printf '\n%s\n' "$(gray '### Summary ###')"
printf '  %s passed, %s failed\n' "$(green $PASS)" "$(if [ $FAIL -gt 0 ]; then red $FAIL; else printf '%d' $FAIL; fi)"

exit $FAIL
