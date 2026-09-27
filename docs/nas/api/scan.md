# 媒体扫描 API

媒体扫描是**异步**的：mutation 立即返回，扫描跑在 `tokio::spawn` 后的 background task，进度通过 WebSocket `media_scan_progress` 事件推送给前端。

## Query

### `scanProgress: ScanProgress!`（顶层查询）

初始渲染 / 轮询用；payload 与 `media_scan_progress` WS 推送相同。

```graphql
{ scanProgress { indexed pending total state } }
```

字段：

| 字段 | 类型 | 说明 |
|------|------|------|
| `indexed` | `Long!` | 已索引文件数 |
| `pending` | `Long!` | `max(total - indexed, 0)` |
| `total` | `Long!` | 总文件数（precount 后才知道；precount 期间是 0） |
| `state` | `ScanState!` 枚举 | `IDLE` / `RUNNING` / `PAUSED` / `STOPPED`（GraphQL 与 `media:scan:progress` WS payload 同值域）|

## Mutation

### `startMediaScan(root: String!): Boolean!`

启动 / 恢复一个 scan。**如果是 paused 状态，恢复；否则新开一个**。

```graphql
mutation { startMediaScan(root: "/mnt/data") }
```

### `pauseMediaScan: Boolean!`

暂停。**立即** publish 一个 progress 事件（state=PAUSED）让前端无延迟反应。

```graphql
mutation { pauseMediaScan }
```

### `resumeMediaScan: Boolean!`

从 paused 恢复。

```graphql
mutation { resumeMediaScan }
```

### `stopMediaScan: Boolean!`

完全停止。**立即** publish 一个 `{state: "STOPPED"}` 事件。

```graphql
mutation { stopMediaScan }
```

### `rebuildMediaIndex(root: String!): Boolean!`

**从零开始重建** media index。同步发一个 `{state: "RUNNING", root, indexed:0, total:0}` 事件，然后 spawn 一个后台 task：abort 老 scan → 清空 KV → precount → 1Hz ticker publish → walk + scan。**不阻塞 API**。

```graphql
mutation { rebuildMediaIndex(root: "/") }
```

curl：

```bash
curl -s -X POST -H "Authorization: Bearer dev" \
  -H "Content-Type: application/json" \
  http://127.0.0.1:8080/graphql \
  -d '{"query":"mutation { rebuildMediaIndex(root: \"/tmp\") }"}'
```

## WebSocket 事件

`ws_hub` 订阅 `media:scan:progress`，推 msg_type=4 给所有已连 WS 客户端。

事件 payload：

```json
{
  "indexed": 1500,
  "pending": 1373,
  "total": 2873,
  "state": "RUNNING",
  "root": "/tmp"
}
```

| 字段 | 何时存在 |
|------|----------|
| `root` | scan 进行中（ticker 期间 + final idle event） |
| | **不**在 `stopped` event 里 |

事件流（一次完整的 rebuild）：

1. **immediate** `{state:"RUNNING", root, indexed:0, total:0}`（在 mutation 返回**前**同步发）
2. ~1s 后 precount 完成：`{state:"RUNNING", root, indexed:0, total:N}`
3. **每 1 秒一次 ticker**：`{state:"RUNNING", root, indexed:k, total:N}`（k 单调递增）
4. **walk 结束**：`{state:"IDLE", root, indexed:N, total:N}`
5. 用户点 stop 时再发：`{state:"STOPPED", indexed:k', total:N'}`（**无 root**）

## 客户端代码

```ts
// web/src/App.vue
ws.onmessage = async (event) => {
  // 解密 / 解析成 {type, data}
  if (type === 'media_scan_progress') {
    scanProgress.value = data  // {indexed, pending, total, state, root}
  }
}
```

## 诊断 log

启动时设 `RUST_LOG=info`（或 config.toml 里 `log.level = "info"`），能看到：

```
INFO [scan] start_walk_and_scan enter root=/tmp
INFO [scan] scanner state set: running root=/tmp
INFO [scan] spawning 1s ticker task
INFO [scan] spawning precount on blocking pool
INFO [scan] ticker task started
INFO [scan] precount done total=2873
INFO [scan] publish_progress indexed=0 pending=2873 total=2873 state=running root_present=true
INFO [ws_hub] forwarding scan event to cid=... payload=...
INFO [scan] spawning walk task on blocking pool
INFO [scan] start_walk_and_scan return
INFO [scan] walk task started root=/tmp
INFO [scan] walk done files_seen=2873 indexed=2873
INFO [scan] publish_progress indexed=2873 pending=0 total=2873 state=idle root_present=true
INFO [ws_hub] forwarding scan event to cid=... payload=...
INFO [scan] walk task finished, state=idle
INFO [scan] ticker notified, exit
INFO [scan] ticker task ended
```
