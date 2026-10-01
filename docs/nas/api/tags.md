# 标签 API

对齐 plain-app 契约：**tag relation 的 key = 媒体 id**（GraphQL `audios`/`images`/`videos`
返回的 `id`，即媒体行 uuid），写入和读取都按 id，且按 `DataType` 过滤
（对应 plain-app `getMediaIds(type, query)` / `TagsLoader.load(id, type)`）。

`DataType` 枚举：`Default` / `Audio` / `Video` / `Image`（数字 kind：0/1/2/3）。

## Query

### `tags(type: DataType!): [Tag!]!`

```graphql
{ tags(type: Audio) { id name type count } }
```

curl：

```bash
curl -s -X POST -H "Authorization: Bearer dev" \
  -H "Content-Type: application/json" \
  http://127.0.0.1:8080/graphql \
  -d '{"query":"{ tags(type: Audio) { id name count } }"}'
```

`count` = 该 tag 的 relation 数（每个媒体 id 一条）。

### 媒体列表 / `fileInfo` 的 `tags` 字段

`audios` / `images` / `videos` 每项的 `tags` 与 `fileInfo(id, path)` 的 `tags`
都按 **媒体 id** 查 relation，并只返回 `type` 与查询类型一致的 tag
（音频项不会显示 IMAGE tag）。`fileInfo` 传空 `id` 时 `tags` 为空
（plain-app 同语义：只有已索引媒体才有 tag）。

## Mutation

### `createTag(type: DataType!, name: String!): Tag!`

```graphql
mutation { createTag(type: Audio, name: "favorite") {
  id name type count
} }
```

### `updateTag(id: ID!, name: String!): Tag!`

```graphql
mutation { updateTag(id: "tag-1", name: "favourites") { id name } }
```

### `deleteTag(id: ID!): Boolean!`

会同时删所有关联。

```graphql
mutation { deleteTag(id: "tag-1") }
```

### `addToTags(type: DataType!, tagIds: [String!]!, query: String!): Boolean!`

批量加 tag（复刻 plain-app `addToTags`）。`query` 是共享搜索 DSL：

- 前端勾选若干条目时为 `ids:<id1>,<id2>,…`（显式 id 列表，不查索引）；
- 「全选」时为页面当前 query（如空串、`trash:false`、自由文本），后端按
  `type` 限定到媒体索引解析出匹配的媒体 id（上限 10000，与 Go 版一致）。

每个解析出的媒体 id 存一条 relation（key=id）；tag 已有的 id 跳过（plain-app 同语义，无重复行）。

```graphql
mutation { addToTags(
  type: Audio
  tagIds: ["tag-1"]
  query: "ids:<audio-id-1>,<audio-id-2>"
) }
```

### `updateTagRelations(type: DataType!, item: TagRelationStub!, addTagIds: [String!]!, removeTagIds: [String!]!): Boolean!`

单条编辑。`item.key` = 媒体 id（前端传的就是列表项的 `id`）。

```graphql
mutation { updateTagRelations(
  type: Audio
  item: { key: "<audio-id>", title: "Song", size: 1234 }
  addTagIds: ["tag-1"]
  removeTagIds: ["tag-old"]
) }
```

`TagRelationStub`：

```graphql
input TagRelationStub {
  key: String!
  title: String!
  size: Long!
}
```

### `removeFromTags(type: DataType!, tagIds: [String!]!, query: String!): Boolean!`

与 `addToTags` 同一套 query 解析（`ids:…` 或页面 query → 媒体 id），逐 id 删关联。

```graphql
mutation { removeFromTags(
  type: Audio
  tagIds: ["tag-1"]
  query: "ids:<audio-id>"
) }
```

## 存储

SQLite `<data_dir>/plain.db`（plain-rs `library` feature，与 plain-desktop
共用同一套行为代码）：

```
tags             (id TEXT PRIMARY KEY, type INTEGER, name TEXT)
tag_relations    (tag_id, key) — 主键 (tag_id, key)，key 上有索引
```

`count` 不落盘，读取时子查询实时计算（`SELECT COUNT(*) FROM tag_relations
WHERE tag_id = ?`）。
