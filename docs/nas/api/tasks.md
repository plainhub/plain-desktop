# 文件任务 API

异步 copy / move。前端调 mutation 拿 `FileTask` (status=QUEUED)，worker 跑，进度通过 WS 推送。

## 模型

```graphql
enum FileTaskType { Copy Move }
enum FileTaskStatus { Queued Running Done Error }

type FileTask {
  id: ID!
  type: FileTaskType!
  title: String!
  status: FileTaskStatus!
  error: String!
  totalBytes: Long!   # 字节数一律 Long（API_SPEC §1；曾因 i32 截断 >2GiB 进度）
  doneBytes: Long!
  totalItems: Int!
  doneItems: Int!
  createdAt: Instant!
  updatedAt: Instant!
}

input FileTaskOpInput {
  src: String!
  dst: String!
  overwrite: Boolean!
}
```

## Query

### `fileTasks: [FileTask!]!`

列出**当前 client** 的所有 tasks（dev 模式下返回全部）。原名 `getTasks`，
GraphQL 不用 get 前缀（API_SPEC §7 命名）。

```graphql
{ fileTasks { id type title status totalBytes doneBytes totalItems doneItems } }
```

## Mutation

### `createCopyTask(ops: [FileTaskOpInput!]!): FileTask!`

```graphql
mutation { createCopyTask(ops: [
  { src: "/tmp/a.txt", dst: "/mnt/b/a.txt", overwrite: false }
]) { id status title } }
```

### `createMoveTask(ops: [FileTaskOpInput!]!): FileTask!`

```graphql
mutation { createMoveTask(ops: [
  { src: "/tmp/a.txt", dst: "/mnt/b/a.txt", overwrite: true }
]) { id status title } }
```

## WebSocket 事件

`ws_hub` 订阅 `file:task:progress`，**带 cid filter**（每个 client 只收自己的），推 msg_type=6。

Payload 字段（最小集）：

```json
{
  "id": "task-uuid",
  "status": "RUNNING",
  "doneBytes": 12345,
  "totalBytes": 100000,
  "doneItems": 1,
  "totalItems": 3
}
```

事件流：
1. 创建时立即推送 `{status: "QUEUED"}`
2. worker 开始时 `{status: "RUNNING"}`
3. 每 ~200ms 推送 `{status: "RUNNING", doneBytes, totalBytes}`
4. 结束 `{status: "DONE"}` 或 `{status: "ERROR", error: "..."}`

## 客户端代码

```ts
// web/src/stores/tasks.ts 里的 handleFileTaskProgress
function handleFileTaskProgress(data: any) {
  const idx = tasks.findIndex(t => t.id === data.id)
  if (idx >= 0) tasks[idx] = { ...tasks[idx], ...data }
  else tasks.unshift(data)
}
```
