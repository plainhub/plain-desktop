# 回收站 API

## 模型

每个文件或目录移入回收站时会存到对应 disk 的 `.plain-trash/<bucket>/<file>` 下，元数据写 KV store：

```graphql
enum TrashedFileType { FILE DIR }

type TrashedFile {
  id: ID!              # trash id (uuid)
  type: TrashedFileType! # FILE | DIR
  originalPath: String! # 删除前的原始路径
  disk: String!         # 磁盘根（"/" 或 "/mnt/data"）
  trashRelPath: String! # 相对 disk 的 trash 路径
  deletedAt: Instant!   # RFC3339 UTC（API_SPEC §1）
  uid: Int!
  gid: Int!
  mode: Int!
  sizeBytes: Long       # 文件大小（字节；仅 file）
  entryCount: Int       # 目录项数（仅 dir）
  displayName: String!  # originalPath 的 base name
  trashedPath: String!  # 物理路径 = `<disk>/<trashRelPath>`
}
```

## Query

### `trashedFileCount: Int!`

```graphql
{ trashedFileCount }
```

### `trashedFiles(offset: Int!, limit: Int!, query: String!, sortBy: TrashedFileSortBy!): [TrashedFile!]!`

`query` 是共享 DSL（API_SPEC §3/§5，空串 = 不过滤）；`sortBy` 必填。

```graphql
{ trashedFiles(offset: 0, limit: 50, query: "", sortBy: DATE_DESC) {
  id type originalPath displayName trashedPath sizeBytes deletedAt
} }
```

`TrashedFileSortBy` 枚举：`DATE_ASC` / `DATE_DESC` / `SIZE_ASC` / `SIZE_DESC` / `NAME_ASC` / `NAME_DESC`。

curl：

```bash
curl -s -X POST -H "Authorization: Bearer dev" \
  -H "Content-Type: application/json" \
  http://127.0.0.1:8080/graphql \
  -d '{"query":"{ trashedFiles(offset: 0, limit: 10, query: \"\", sortBy: DATE_DESC) { id originalPath displayName } }"}'
```

## Mutation

### `trashFiles(paths: [String!]!): ActionResult!`

把若干 path 移到 `.plain-trash`。文件 / 目录都可以。**同时从 media index 删**。
批量破坏类操作返回 `ActionResult{affectedCount}`（API_SPEC §6）——实际移入
trash 的条目数。

```graphql
mutation { trashFiles(paths: ["/tmp/a.txt", "/tmp/dir"]) { affectedCount } }
```

### `restoreFiles(paths: [String!]!): ActionResult!`

从 trash 恢复。`paths` 可以是物理 trash 路径（`trashedPath`）或者 trash id。
`affectedCount` = 实际恢复的条目数。

```graphql
mutation { restoreFiles(paths: ["/mnt/data/.plain-trash/data/2026/06/f_abc", "trash-id-xyz"]) { affectedCount } }
```

### `deleteTrashedFile(path: String!): Boolean!`

**永久**删除一个 trash 条目（不进入二次回收，物理文件也删）。单条幂等删除，
按 API_SPEC §6 返回 `Boolean!`。

```graphql
mutation { deleteTrashedFile(path: "trash-id-xyz") }
```

错误：

| 错误 | 含义 |
|------|------|
| `trash entry not found` | id 找不到 |

## 注意

- trash bucket layout：`.plain-trash/data/YYYY/MM/f_<hash>_<name>` — 同一月内同 hash 冲突会自动加序号。
- restore 失败时原 trash 条目仍保留（不会丢失文件）。
