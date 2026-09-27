# 文件 API

## Query

### `files(root: String!, offset: Int!, limit: Int!, query: String!, sortBy: FileSortBy!): [File!]!`

列出文件 / 搜索（对齐 plain-app 方言）。生效目录优先级：query DSL 的
`parent:` → `root` 参数 → DSL 遗留 `root_path:`；DSL 遗留 `relative_path:`
拼接在其下。`text:` 走 tantivy 递归搜索，`trash:true` 走回收站列表，
`show_hidden:true` 包含隐藏项。

```graphql
{ files(root: "/mnt", offset: 0, limit: 100, query: "", sortBy: NameAsc) {
  name path isDir createdAt updatedAt size children mediaId
} }
```

curl：

```bash
curl -s -X POST -H "Authorization: Bearer dev" \
  -H "Content-Type: application/json" \
  http://127.0.0.1:8080/graphql \
  -d '{"query":"{ files(root: \"/mnt\", offset: 0, limit: 50, query: \"\", sortBy: NameAsc) { name path isDir size children } }"}'
```

`FileSortBy` 枚举：`DateAsc` / `DateDesc` / `SizeAsc` / `SizeDesc` / `NameAsc` / `NameDesc`。

### `filesCount(query: String!): Int!`

总文件数（搜索用）。**当前 stub：固定返回 0**。

### `recentFiles: [File!]!`

最近访问过的文件。**当前 stub：固定返回空数组**。

### `recentFilesCount: Int!`

最近文件数。**当前 stub：固定返回 0**。

### `fileInfo(id: ID, path: String!, includeDirSize: Boolean = false): FileInfo`

单个文件的详细信息（用于 lightbox UI）。

```graphql
{ fileInfo(path: "/mnt/data/photo.jpg") {
  path updatedAt size tags { id name }
  data {
    __typename
    ... on ImageFileInfo { width height location { latitude longitude } }
    ... on VideoFileInfo { durationMs width height location { latitude longitude } }
    ... on AudioFileInfo { durationMs location { latitude longitude } }
  }
} }
```

curl：

```bash
curl -s -X POST -H "Authorization: Bearer dev" \
  -H "Content-Type: application/json" \
  http://127.0.0.1:8080/graphql \
  -d '{"query":"{ fileInfo(path: \"/tmp/some.mp3\") { path size tags { name } } }"}'
```

注意：`id` 参数当前**未使用**（media index 还没接入），用 `path` 即可。

### `pathExists(path: String!): Boolean!`

路径是否存在。空白/`.` 返回 `false`；stat 出错（如权限不足）一律按"不存在"处理——**全谓词、不报错**。

```graphql
{ pathExists(path: "/tmp") }
```

### `pathKind(path: String!): PathKind`

路径种类：`FILE` / `DIR`；**null = 不存在**（与 `pathExists` 同语义）。

```graphql
{ pathKind(path: "/tmp") }

# alias 批量：一个请求探测多个路径
{ a: pathKind(path: "/tmp") b: pathKind(path: "/nope") }
```

```graphql
{ pathStats(paths: ["/tmp", "/missing"]) {
  path exists isDir
} }
```

## Mutation

### `createDir(path: String!): File!`

新建目录。返回新建的 `File`。

```graphql
mutation { createDir(path: "/tmp/test") {
  path isDir createdAt updatedAt
} }
```

错误：

| 错误 | 含义 |
|------|------|
| `bad path` | path 没有 parent |

### `writeTextFile(path: String!, content: String!, overwrite: Boolean!): File!`

写文本文件。**2 MiB 容量上限**。path 已存在时 `overwrite=false` → 报 `target exists`；path 是目录 → 报 `path is a directory`。

```graphql
mutation { writeTextFile(
  path: "/tmp/note.txt"
  content: "hello world"
  overwrite: false
) { path size updatedAt } }
```

### `renameFile(path: String!, name: String!): Boolean!`

单文件改名（不能跨目录；要 move 用 `moveFile`）。

```graphql
mutation { renameFile(path: "/tmp/old.txt", name: "new.txt") }
```

### `copyFile(src: String!, dst: String!, overwrite: Boolean!): Boolean!`

单文件 copy。

```graphql
mutation { copyFile(src: "/tmp/a.txt", dst: "/tmp/b.txt", overwrite: true) }
```

### `moveFile(src: String!, dst: String!, overwrite: Boolean!): Boolean!`

单文件 move（rename + 必要时跨设备 copy + delete）。

```graphql
mutation { moveFile(src: "/tmp/a.txt", dst: "/mnt/b/a.txt", overwrite: false) }
```

### `deleteFiles(paths: [String!]!): ActionResult!`

返回 `{affectedCount}`：实际删除的路径数（逐路径 best-effort，不存在的路径不计）。

批量删文件 / 目录。**拒绝删 `/` 根**。如果路径是媒体文件，会同步从 media index 删条目。

```graphql
mutation { deleteFiles(paths: ["/tmp/a.txt", "/tmp/dir"]) { affectedCount } }
```

错误：

| 错误 | 含义 |
|------|------|
| `refusing to delete root` | paths 里有 `/` |
