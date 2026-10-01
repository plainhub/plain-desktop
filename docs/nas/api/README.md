# API 概览

PlainNAS 后端对外暴露两类 API：

1. **GraphQL**（`POST /graphql`）— 浏览器 / SPA 主要入口；schema 由 `async-graphql` 自动生成。
2. **HTTP**（`POST /auth`, `POST /auth/status`, `POST /auth/setup`, `GET/POST /upload`, `GET/POST /upload_chunk`, `GET /ws`, `GET /health_check`, `GET /fs`, `GET /media/:name`, `GET /zip/dir`, `GET /zip/files`）— 二进制上传、WebSocket、静态文件。

本目录对 **每个 GraphQL 字段 + 关键 HTTP 路由** 提供：
- 完整字段名、参数、返回类型（与 `async-graphql` 生成的 schema 字节级一致）
- 至少一个 `curl` 调用示例
- 失败模式 / 错误码

## 启动与认证

### 1. 构建后端

```bash
cargo build        # debug 构建，产物在 ./target/debug/plain-nas
# 或：
cargo build --release
```

### 2. 启动服务（开发模式，非 root）

```bash
PLAIN_NAS_ALLOW_NONROOT=1 \
PLAIN_NAS_DATA_DIR=./tmp-data \
RUST_LOG=info \
./target/debug/plain-nas run
```

- 默认 HTTP 端口 **8080**，HTTPS 端口 **8443**（端口来自 `/etc/plainnas/config.toml` 的 `server.http_port` / `server.https_port`，见 [src/cmd/install/config.toml](../../src/cmd/install/config.toml)）
- `PLAIN_NAS_ALLOW_NONROOT=1` 跳过 root 检查（仅开发用）
- `PLAIN_NAS_DATA_DIR=./tmp-data` 把运行时数据放到当前目录（默认 `/var/lib/plainnas` 需要 root）
- 需要默认端口 / 默认数据目录 / 磁盘挂载等 root 能力时，改用：`cargo build && sudo -E env "PATH=$PATH" ./target/debug/plain-nas run`

### 3. 配置 dev token

`/etc/plainnas/config.toml`（或自定义路径）里：

```toml
[auth]
dev_token = "dev"   # 默认空；设为 "dev" 后即可用 Authorization: Bearer dev 调用 /graphql
```

如果配置文件不存在或 `dev_token` 为空，dev 模式不可用，必须走完整 session 流程（见 `auth.md`）。开发时最简单做法是手动在 config.toml 里加上 `dev_token = "dev"`。

### 4. 验证

```bash
# 健康检查
curl -s http://127.0.0.1:8080/health_check
# => ok

# GraphQL（dev 模式）
curl -s -X POST -H "Authorization: Bearer dev" \
  -H "Content-Type: application/json" \
  http://127.0.0.1:8080/graphql \
  -d '{"query":"{ app { httpPort httpsPort urlToken } }"}'
```

### 两种鉴权模式

| 模式 | 触发条件 | 调用方式 | Body 加密 |
|------|----------|----------|-----------|
| **Session** | `c-id: <session_cid>` header 存在 | `/auth` 拿 session token；之后 `/graphql` body 用 XChaCha20-Poly1305 加密 | ✅ |
| **Dev** | `Authorization: Bearer <dev_token>` header（config.toml 里 `auth.dev_token`） | 直接调用 `/graphql`；client_id 固定为 `"dev"` | ❌ plaintext JSON |

**所有文档示例都走 dev 模式**（最简单）。生产模式见 `auth.md`。

## 章节

| 文件 | 内容 |
|------|------|
| `auth.md` | `/auth`, `/auth/status`, `/auth/setup` 三个 HTTP 端点 + 完整 session 流程 |
| `app.md` | `app`, `deviceInfo`, `appUpdate` 三个 query + `setDeviceName`, `updateDeviceName`, `setTempValue` 等 mutation |
| `storage.md` | `mounts`, `disks` query + `formatDisk`, `setMountAlias` mutation + StorageMount / StorageDisk 字段 |
| `files.md` | `files`, `fileInfo`, `pathExists`, `pathKind`, `filesCount`, `recentFiles` query + `createDir`, `writeTextFile`, `renameFile`, `copyFile`, `moveFile`, `deleteFiles` mutation + File 字段 |
| `trash.md` | `trashedFileCount`, `trashedFiles` query + `trashFiles`, `restoreFiles`, `deleteTrashedFile` mutation + TrashedFile 字段 |
| `tags.md` | `tags` query + `createTag`, `updateTag`, `deleteTag`, `addToTags`, `updateTagRelations`, `removeFromTags` mutation + Tag / TagRelationStub 字段 |
| `media.md` | `audios`, `audioCount`, `images`, `imageCount`, `videos`, `videoCount`, `mediaBuckets`, `recentFilesCount`, `mediaSourceDirs` query + `trashMediaItems`, `restoreMediaItems`, `deleteMediaItems` mutation + Audio/Image/Video/MediaBucket 字段 |
| `scan.md` | `startMediaScan`, `pauseMediaScan`, `resumeMediaScan`, `stopMediaScan`, `rebuildMediaIndex` mutation + WebSocket `media_scan_progress` 事件 + ScanProgress 字段 |
| `tasks.md` | `getTasks` query + `createCopyTask`, `createMoveTask` mutation + FileTask 字段 + WebSocket `file_task_progress` 事件 |
| `chat.md` | 聊天栈（plain-app 契约）：GraphQL 查询/变更、ChatStatus 语义、配对协议、plain.db 存储设计 |
| `favorites.md` | `favoriteFolders` query + `addFavoriteFolder`, `removeFavoriteFolder`, `setFavoriteFolderAlias` mutation + FavoriteFolder 字段 |
| `playlist.md` | plain-app 音频播放面全套：`audioQueueItems`, `audioQueueItemCount`, `audioLyrics`, `audioPlaylists`, `audioPlaylistItems`, `audioPlaylistItemCount`, `audioPlayHistory` query + `playAudio`, `addPlaylistAudios`, `reorderPlaylistAudios`, `deletePlaylistAudio`, `clearAudioPlaylist`, `updateAudioPlayMode`, `createAudioPlaylist`, `renameAudioPlaylist`, `deleteAudioPlaylist`, `addAudioPlaylistItems`, `removeAudioPlaylistItem`, `playAudioPlaylist`, `playAllAudios` mutation + PlaylistAudio/AudioPlaylist/AudioPlayHistory/MediaPlayMode |
| `chunked-upload.md` | `uploadedChunks` query + `mergeChunks` mutation + `POST /upload`, `POST /upload_chunk` HTTP 端点 |
| `samba.md` | `sambaSettings` query + `setSambaSettings`, `setSambaUserPassword` mutation + SambaSettings 字段 |
| `dlna.md` | `dlnaRenderers` query + `dlnaCast` mutation + DlnaRenderer 字段 + WebSocket `dlna_renderer_found` / `dlna_discovery_done` 事件 |
| `events.md` | `sessions`, `events` query + `logout`, `revokeSession` mutation + Session / Event 字段 |
| `kv.md` | `setTempValue` mutation + TempValue 字段（内存临时值） |
| `developer.md` | `/developer/*` 子页面：`appLogs`/`appLogPath`/`clearAppLogs`、`dataStore*`、`dbTables`/`dbTableRows`/`dbTableInfo`/`dbTableRowCount`/`deleteDbTableRows`、`battery`（惰性） |
| `websocket.md` | WebSocket 协议总览（连接、握手、订阅事件、心跳、关闭） |
| `errors.md` | 通用错误格式 + 常见错误码 |

## 通用约定

### 时间戳

所有 `createdAt` / `updatedAt` / `lastActive` 字段都是 **ISO 8601 / RFC 3339 字符串**（`chrono::DateTime<Utc>::to_rfc3339()`）。

```text
2026-06-09T15:42:00.123456789Z
```

### 路径

- 路径都是**绝对路径**（`/` 开头）。
- 跨平台 normalize 行为：Windows 风格的 `\` 在 index 写入时被 `path_to_slash` 转成 `/`，但 HTTP body 里我们**始终用 `/`**。
- 不存在的路径：`pathExists` 返回 `false`、`pathKind` 返回 `null`——全谓词，**不报错**。

### ID 命名

- Rust 端默认字段是 snake_case（`disk_id`），async-graphql 自动 camelCase（`diskId`）。
- 例外：SDL 里有“全大写”约定（`diskID`、`hasPassword` 等）的字段用 `#[graphql(name = "...")]` 显式改名，保持与前端 schema 一致。
- 详细对照见 `errors.md` 第 3 节"已知 field name 差异"。

### 错误响应

GraphQL 错误统一格式：

```json
{
  "data": null,
  "errors": [{
    "message": "human-readable error",
    "locations": [{"line": 1, "column": 1}],
    "path": ["mounts"]
  }]
}
```

HTTP 错误（auth、upload 等）：

```json
{"errors": [{"message": "..."}]}
```

详细列表见 `errors.md`。
