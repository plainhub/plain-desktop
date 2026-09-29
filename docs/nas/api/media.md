# 媒体库 API

字段契约以手机端 `plain-app/shared/apitest/schema.graphqls` 为唯一权威；
`Instant` 为 RFC3339 UTC 字符串（`2026-09-20T12:34:56.789Z`），`Long` 为 64 位标量（JSON number）。

## 模型

```graphql
type Audio {
  id: ID!
  title: String!
  artist: String!
  path: String!
  durationMs: Long!   # 毫秒（存储层是秒，出口统一 ×1000）
  size: Long!
  bucketId: ID!
  albumFileId: ID!
  createdAt: Instant!
  updatedAt: Instant!
  isFavorite: Boolean!  # NAS 无单媒体收藏，恒 false
  tags: [Tag!]!
}

type Image {
  id: ID!
  title: String!
  path: String!
  size: Long!
  bucketId: ID!
  createdAt: Instant!
  updatedAt: Instant!
  takenAt: Instant      # 无 EXIF 提取，暂镜像 mtime
  isFavorite: Boolean!
  tags: [Tag!]!
}

type Video {
  id: ID!
  title: String!
  path: String!
  durationMs: Long!
  size: Long!
  bucketId: ID!
  createdAt: Instant!
  updatedAt: Instant!
  takenAt: Instant
  isFavorite: Boolean!
  tags: [Tag!]!
}

type Doc {
  id: ID!
  title: String!
  path: String!
  extension: String!   # 小写扩展名（plain-app getFilenameExtension）
  size: Long!
  bucketId: ID!
  createdAt: Instant!
  updatedAt: Instant!
  tags: [Tag!]!
}

type DocExtGroup {
  ext: String!    # 大写展示标签（plain-app 同款），查询时大小写不敏感
  count: Int!
}

type MediaBucket {
  id: ID!
  name: String!
  itemCount: Int!
  topItemPaths: [String!]!
}

type ActionResult {
  affectedCount: Int!
}
```

## Query

### `audios(offset: Int!, limit: Int!, query: String!, sortBy: FileSortBy!): [Audio!]!`

```graphql
{ audios(offset: 0, limit: 50, query: "", sortBy: DATE_DESC) { id title artist } }
```

### `audioCount(query: String!): Int!`

### `images / imageCount / videos / videoCount`

同上（`sortBy` 均必填；`TAKEN_AT_DESC` 在 NAS 上按 DATE_DESC 处理）。

### `docs / docCount / docExtGroups`（文档库，plain-app DocGraphQL 对齐）

文档 = Android MediaStore 可见性规则过滤后，MIME 属于 `text/*` 或 PlainApp Android
`DocMediaStoreHelper.extraDocumentMimeTypes` 白名单（pdf / doc / docx / xlsx / js），
且文件大小大于 0。JSON/XML 不是额外白名单项；仅当 Android MIME 推导结果属于 `text/*`
时才进入 Docs。未知 MIME 留在 Files。各媒体页的纳入/排除规则见
[`media-items.md`](../media-items.md#android-mediastore-compatibility-what-each-media-page-shows)。

```graphql
{ docs(offset: 0, limit: 50, query: "ext:pdf", sortBy: DATE_DESC) { id title extension } }
{ docCount(query: "") }        # 含已 trash（plain-app 语义）
{ docExtGroups { ext count } } # 无参数：全库按扩展名分组，含已 trash
```

`docExtGroups` 候选来自索引 `ext` 字段词典（仅 doc 行携带），每扩展名走
counting collector 精确计数（与删除位图一致），成本只随去重扩展名数量增长；
结果按 ext 升序。侧栏 `ext:` 过滤大小写不敏感（SQLite LIKE 同款）。

### `mediaBuckets(type: MediaDataType!): [MediaBucket!]!`

```graphql
{ mediaBuckets(type: AUDIO) { id name itemCount topItemPaths } }
```

DOC 参与分桶（`bucketed_type` 含 `doc`）。

### `mediaSourceDirs: [String!]!`

```graphql
{ mediaSourceDirs }
# 返回: ["/mnt/data/photos", "/mnt/data/music"]  来自 setMediaSourceDirs 配置
```

curl：

```bash
curl -s -X POST -H "Authorization: Bearer dev" \
  -H "Content-Type: application/json" \
  http://127.0.0.1:8080/graphql \
  -d '{"query":"{ mediaSourceDirs }"}'
```

## Mutation

### `setMediaSourceDirs(dirs: [String!]!): Boolean!`

设置媒体扫描的源目录白名单。空 = 全扫。

```graphql
mutation { setMediaSourceDirs(dirs: ["/mnt/data/photos", "/mnt/data/music"]) }
```

### `trashMediaItems(type: MediaDataType!, query: String!): ActionResult!`

把 query 选中的媒体移入所在盘的 `.nas-trash`；返回实际处理条数。

> **空 query 守卫（API_SPEC §5）**：`trashMediaItems` / `restoreMediaItems` /
> `deleteMediaItems` / `moveMediaItems` 四个按 `query` 编址的批量操作，
> 空串/纯空白 query 一律报 `bulk_query_required`——全量意图必须显式发
> `all:true`（选区构建会忽略该字段，退化为不过滤）。

```graphql
mutation { trashMediaItems(type: AUDIO, query: "size:>10MB") { affectedCount } }
```

### `restoreMediaItems(type: MediaDataType!, query: String!): ActionResult!`

从回收站恢复（query 隐含 `trash:true`）。

### `deleteMediaItems(type: MediaDataType!, query: String!): ActionResult!`

永久删除（物理删除 + 索引行清除）。

### `moveMediaItems(type: MediaDataType!, query: String!, destDir: String!): ActionResult!`

移动到目标目录并重写索引/DB 行。
