# WebSocket 协议

`GET /?cid=<client_id>` 建立连接，用于接收 server-push 事件。

## 两种模式

### Session 模式

```text
1. 客户端连 ws://host:port/?cid=<client_id>
2. 收到 open 事件后，发送**一个二进制 frame**（握手）：
   - 24 byte 随机 nonce
   - ChaCha20-Poly1305 加密的任意 plaintext（前端用当前时间字符串）
3. 服务端用 session key 解密（key = base64-decode session.token）
4. 解密成功 → ws_hub 订阅事件；失败 → 立即 close
5. server 推回加密的事件 frame
```

### Dev 模式

**dev 模式没有握手**（`/graphql` 走 dev token 路径；ws 也走 dev 路径自动 accept）。但实际 Rust 端 `handle_socket` 总是等一个 binary 帧再 register——**所以即使是 dev 模式，ws 连接也要发一个握手 frame**，否则 ws 不会 deliver 事件。

**dev 模式下 client_id 固定为 `"dev"`，但 ws 握手用的是 session key 路径**——而 dev 模式没有 session！

**结论**：dev 模式下 ws 实际上**无法工作**（必须用 session 模式）。**所以 ws 测试需要先调 `/auth` 拿 session**。

## Wire format

```
server → client:  [msg_type: i32 BE] || [nonce: 24] || [chacha20_ct: N] || [tag: 16]
client → server:  [nonce: 24] || [chacha20_ct: N] || [tag: 16]    (only handshake, one frame)
```

`msg_type` 取值：

| 值 | 事件 | 字段 |
|----|------|------|
| 41 | `media_scan_progress` | `{indexed, pending, total, state, root?}` — state 为 `ScanState` 枚举字符串：`IDLE`/`RUNNING`/`PAUSED`/`STOPPED`（与 GraphQL `scanProgress.state` 同值域） |
| 42 | `file_task_progress` | `{id, status, doneBytes, totalBytes, ...}` |
| 43 | `dlna_renderer_found` | `{udn, name, manufacturer, modelName, location}` |
| 44 | `dlna_discovery_done` | `{}` |
| 45 | `disk_format_done` | `{path, ok, error?}` — `formatDisk` 完成广播（所有连接都收，含触发者）；失败时 `ok:false` 带 `error` |
| 1 | `message_created` | `[{id, fromId, toId, channelId, content, createdAt, updatedAt, status, statusData, data}]` — 聊天消息新建/入站（本机发送与 peer 投递共用） |
| 2 | `message_deleted` | `[id]`（单删）或 `ids=a,b,…`（批删，原始字符串 body） |
| 3 | `message_updated` | `[{…同 message_created}]` — 投递结果回写（PENDING→SENT/FAILED/PARTIAL）、重试、链接预览刷新 |
| 18 | `channels_updated` | `[{id, name, ownerId, members:[{peerId,status}], version, status, createdAt, updatedAt}]` — 已加入频道全量列表 |
| 20 | `peer_status_updated` | `{id, online:false}` — deletePeer/unpairPeer |
| 22 | `pairing_request_received` | `PairingEvent` 对象（`{kind:{type:"incomingRequest",request,senderIp}, deviceId, deviceName}`） |
| 23 | `pairing_success` | `PairingEvent` 对象 |
| 24 | `pairing_failed` | `PairingEvent` 对象（`kind.reason` 失败原因） |
| 25 | `pairing_cancelled` | `PairingEvent` 对象 |
| 26 | `pairing_started` | `PairingEvent` 对象 |
| 28 | `channel_invite_received` | `{channelId, channelName, fromId, fromName}` |

注意：msg_type 5 (`plainTypes`) **保留**，body 是不加密的原始字节——目前没用。

## 客户端解析（前端）

前端用 `web/src/lib/api/sjcl-arraybuffer.ts` 的 `parseWebSocketData`：

```ts
export function parseWebSocketData(buffer: ArrayBuffer, plainTypes: number[]): { type: number; data: any } {
  const inView = new DataView(buffer)
  const prefix = inView.getInt32(0)              // big-endian
  if (plainTypes.includes(prefix)) {
    return { type: prefix, data: buffer.slice(4) }
  }
  // 跳过 4 字节 prefix，剩下按 4 字节切成 uint32 BE → out 数组
  // out 数组就是 sjcl.BitArray 格式，喂给 chachaDecrypt(key, out)
  // ...
}
```

`App.vue` 的 ws.onmessage 流程：

```ts
const r = parseWebSocketData(buffer, [5])             // type 5 走原始
const type = EventType[r.type] ?? ''                  // 4→"media_scan_progress", etc.
if (plainTypes.includes(r.type)) {
  emitter.emit(type, new Blob([r.data], { type: 'application/octet-stream' }))
} else {
  const json = chachaDecrypt(key, r.data)              // 解密
  if (type) emitter.emit(type, json ? JSON.parse(json) : null)
}
```

## 端到端 Node.js 测试脚本

`/tmp/ws_e2e.mjs`（参考 plain-nas 仓库根）—— 用 `@noble/ciphers` 的 xchacha20poly1305 模拟前端，验证 Rust 端 ws 推送。

```bash
# 1. 准备 session (需要走 /auth)
# 见 auth.md

# 2. 跑测试
node /tmp/ws_e2e.mjs
# 预期:
# [client] ws open, sending handshake
# [client] handshake sent, blob size 53
# [client] sending rebuildMediaIndex
# [client] #1 type=media_scan_progress payload={...state:"RUNNING",total:0...}
# [client] rebuild result: {"data":{"rebuildMediaIndex":true}}
# [client] #2 type=media_scan_progress payload={...total:N...}
# [client] #3 type=media_scan_progress payload={...indexed:9...}
# ...
# [client] #N type=media_scan_progress payload={...indexed:N,state:"idle"...}
```

## 心跳

**没有 ping/pong 协议**。前端用 `WebSocket` 自带的 keepalive（浏览器自动）。如服务端检测到 close，subscription 自动 unsubscribe（`unregister` 在 handle_socket 退出时调用）。

## 关闭

- 客户端 close → 服务端 reader task 收到 `None` → 调 `unregister(cid)` 清订阅 + 关闭 socket
- 服务端主动 close → 发 `Message::Close(None)`
