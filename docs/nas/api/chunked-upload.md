# 分片上传 API

## `mergeAppFileChunks`（聊天附件）

`mergeAppFileChunks(fileId, totalChunks, fileName, totalSize): MergeTask!`
与 `mergeChunks` 同一套任务表（`mergeStatus` 轮询），但合并结果导入
内容寻址附件存储（`<data_dir>/files/{aa}/{bb}/{hash}.{ext}`，SHA-256
去重；文件名扩展名决定落盘扩展名）。`DONE` 的 `value` 是 fid 后缀
`"{hash}.{ext}"`，前端用它拼 `fid:` URI 发送聊天消息；`/fs?id=` 端到端
解出该 URI 并从存储流式返回（见 `chat.md`）。

## Query

### `uploadedChunks(fileId: ID!): [String!]!`

返回已上传的 chunk index 列表（前端断点续传时用）。

```graphql
{ uploadedChunks(fileId: "upload-abc123") }
# [0, 1, 2, 5, 6]   <- 3 和 4 还没传
```

curl：

```bash
curl -s -X POST -H "Authorization: Bearer dev" \
  -H "Content-Type: application/json" \
  http://127.0.0.1:8080/graphql \
  -d '{"query":"{ uploadedChunks(fileId: \"upload-abc\") }"}'
```

## HTTP 端点

### `POST /upload`

单次上传（不走分片）。multipart/form-data。

```bash
curl -X POST -H "c-id: <client_id>" \
  -H "Authorization: Bearer <session_token_or_dev>" \
  -F "path=/tmp/upload.bin" \
  -F "file=@./local_file.bin" \
  http://127.0.0.1:8080/upload
```

最大 64 GiB（`DefaultBodyLimit::max(64 * 1024 * 1024 * 1024)`）。

### `POST /upload_chunk`

分片上传一次。

```bash
curl -X POST -H "c-id: <client_id>" \
  -H "Authorization: Bearer <session_token_or_dev>" \
  -F "fileId=upload-abc123" \
  -F "index=5" \
  -F "total=10" \
  -F "chunk=@./part_5.bin" \
  http://127.0.0.1:8080/upload_chunk
```

返回：`{"uploaded": [0, 1, 2, 3, 4, 5]}` （所有已上传的 index）

## Mutation

### `mergeChunks(fileId: ID!, totalChunks: Int!, path: String!, replace: Boolean!, totalSize: Long!): MergeTask!`

把所有 chunk 合并成最终文件。

```graphql
mutation { mergeChunks(
  fileId: "upload-abc123"
  totalChunks: 10
  path: "/tmp/final_file.bin"
  replace: true
) }
# 返回: "final_file.bin"  (实际写入的文件名)
```

错误：

| 错误 | 含义 |
|------|------|
| `chunk <i> missing` | 某个 chunk 没传完 |
| `target exists` | path 已存在且 `replace=false` |
| `path is a directory` | path 是目录 |

## 典型前端流程

```
1. file = pick()
2. slices = split(file, 5MB)
3. fileId = randomUUID()
4. for (i = 0; i < slices.length; i++):
     POST /upload_chunk  {fileId, index:i, total:N, chunk:slice[i]}
5. POST /graphql mutation mergeChunks(fileId, N, dest, true)
```
