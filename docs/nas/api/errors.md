# 错误 / 已知差异

## 错误格式

### GraphQL 错误

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

### HTTP 错误

```json
{"errors": [{"message": "..."}]}
```

## 通用错误码

| 错误 message | 端点 / 场景 |
|--------------|-------------|
| `Unauthorized` | `/auth` 密码错 / 解密失败 |
| `Bad request` | body 是空 / JSON parse 失败 |
| `Missing client id` | `c-id` header 缺失 |
| `Password not configured` | 还没调 `/auth/setup` |
| `Decryption failed` | 加密 body 解不出来（key 不对） |
| `Server misconfigured` | config.toml 里 password 长度不够 32 byte |
| `target exists` | 写文件时 overwrite=false 且 path 已存在 |
| `path is a directory` | path 是目录但当文件用 |
| `refusing to delete root` | deleteFiles 包含 `/` |
| `chunk <i> missing` | mergeChunks 缺 chunk |
| `renderer not found` | dlnaCast 的 udn 不在缓存 |
| `device_name_invalid` | setHostname sanitize 后为空 |
| `device_name_set_hostname_failed` | hostnamectl 退出非 0 |
| `format_disk_failed: <path>: <err>` | formatDisk 失败 |
| `tag missing` | createTag/updateTag 后没找到 |
| `bulk_query_required` | 按 `query` 编址的批量 mutation（delete/trash/restore/moveMediaItems）收到空 query——全量意图必须显式发 `all:true`（API_SPEC §5） |
| `internal error` | KV IO 错误等 |

## 已知 field name 差异

Rust 端 async-graphql 默认 snake_case → camelCase。**少数**字段用了“全大写”（保留字或全大写 ID），需要 Rust 端 `#[graphql(name = "...")]` 显式对齐。

| 字段 | Rust 字段名 | GraphQL 名 | 来源 |
|------|------------|------------|------|
| `StorageMount.diskID` | `disk_id` | `diskID` | SDL `diskID: ID` 显式大写 |
| `FavoriteFolder.rootPath` | `root_path` | `rootPath` | |
| `FavoriteFolder.relativePath` | `relative_path` | `relativePath` | |
| `SambaShare.sharePath` | `share_path` | `sharePath` | |
| `SambaShare.readOnly` | `read_only` | `readOnly` | |
| `SambaSettings.hasPassword` | `has_password` | `hasPassword` | |
| `SambaSettings.serviceName` | `service_name` | `serviceName` | |
| `SambaSettings.serviceActive` | `service_active` | `serviceActive` | |
| `SambaSettings.serviceEnabled` | `service_enabled` | `serviceEnabled` | |
| `DlnaRenderer.modelName` | `model_name` | `modelName` | |
| `MediaBucket.itemCount` | `item_count` | `itemCount` | |
| `MediaBucket.topItems` | `top_items` | `topItems` | |
| `MediaActionResult.type` | `kind` | `type` | SDL 关键字 |
| `Image/Video/Audio.bucketId` | `bucket_id` | `bucketId` | |
| `Image/Video/Audio.createdAt` | `created_at` | `createdAt` | |
| `Image/Video/Audio.updatedAt` | `updated_at` | `updatedAt` | |
| `FileInfo.updatedAt` | `updated_at` | `updatedAt` | |
| `NicInfo.speedRate` | `speed_rate` | `speedRate` | |
| `FileTaskOpInput.overwrite` | `overwrite` | `overwrite` | 已是单字 |

## 已知 stub（还没实现的功能）

| Query / Mutation | 状态 |
|------------------|------|
| `audios` / `audioCount` | 返回空 / 0 |
| `images` / `imageCount` | 返回空 / 0 |
| `videos` / `videoCount` | 返回空 / 0 |
| `mediaBuckets` | 返回空 |
| `recentFiles` / `recentFilesCount` | 返回空 / 0 |
| `filesCount` | 返回 0 |
| `trashMediaItems` / `restoreMediaItems` / `deleteMediaItems` | 回显 type+query 不实际操作 |
| `fileInfo.data` | 永远 `null`（需要 EXIF / ffprobe 集成） |
| ~~`audios` 真实 metadata (artist, duration)~~ | **已解决（2026-09-19）**：读路径惰性探测 + 一次性持久化（Go media_helper 同构），MP4 族走进程内 moov→mvhd 解析（lofty 拒收无音轨 mp4） |

## Long 标量

原始 Go SDL 用 `Long!`（graphql-scalars）。Rust 端用 `i64`（async-graphql 默认映射成 `Int` 32-bit，**有可能 overflow**）。

**实际影响**：磁盘容量 / 文件大小超过 2 GiB 会有问题。**当前 workaround**：所有数值都在 i64 范围内，async-graphql 实际渲染成 `Int`（虽然 schema 应该标 Long）。前端 Apollo 客户端会把它当 number 处理。

未来要修：在 schema 里加 `scalar Long`（参照 graphql-scalars）然后所有 `i64` 字段标 `#[graphql(name="...")]`。
