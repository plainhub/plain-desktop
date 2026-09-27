# Chat API（plain-app 契约对齐）

NAS 上的聊天栈复用 `plain_rs::chat`（与 plain-desktop 同一份实现，源头是 plain-app
Kotlin 的 `ChatManager` / `ChatSender` / `PairingCore` / `ChannelSystemMessage*` 移植）。

- 存储：`<data_dir>/chat.db`（SQLite，plain-app Room 同款聊天表——`chats`、
  `chat_channels`、`peers`、`nearby_device_cache`、`app_files`；共享 core
  同时建 `bookmarks` / `bookmark_groups`，但 NAS 的 Bookmark API 仍使用 fjall KV）。
- 身份：服务器 `client_id` + `/init` 公布的 Ed25519 签名密钥对 + 显示名
  （`device_name` 偏好，缺省回落 hostname）。设备类型线上值 `NAS`。
- 文件 id 加密用 `/fs` 的 URL token，聊天附件与 `/fs` 同一条取数路径。

## GraphQL（全部在手机契约 `plain-app/shared/apitest/schema.graphqls` 内）

Queries：

| 字段 | 说明 |
| --- | --- |
| `peers: [Peer!]!` | 已配对/已知 LAN 设备；`online` 当前恒 false（mDNS 在线跟踪后续补） |
| `chatItems(target, offset, limit, query)` | 单会话分页，最新在前取页、返回旧→新；`target` 为裸/`peer:` 前缀 peer id 或 `channel:<id>`；`query` 的 `text:` 字段子串过滤 |
| `latestChatItems: [ChatItem!]!` | 每会话最新一条（频道/peer/local） |
| `chatChannels: [ChatChannel!]!` | 未退出的频道 |
| `appFiles(offset, limit, query)` / `appFileCount(query)` | 附件存储分页/计数，显示名子串过滤 |

Mutations：

| 字段 | 说明 |
| --- | --- |
| `sendChatItem(target, content)` | 发送：`peer:<id>` 直发（对端共享密钥加密）、`channel:<id>` 频道（选主广播）、其他值=本地笔记；远端目标先落库 `PENDING`，投递结果经 WS `message_updated` 推送 |
| `deleteChatItem(id)` / `deleteChatItems(query)` / `retryChatItem(id)` | 单删/按 DSL 批删（`ids:`/`channel:`/`peer:`，空 query=无操作 affectedCount 0，§5 例外）/重投递 |
| `createChatChannel(name)` … | 频道 CRUD + 成员增删 + 邀请接受/拒绝（owner Ed25519 签名验签、版本乐观并发），`respondChannelInvite(id, accept)` 为 web 端便捷分支 |
| `deletePeer(id)` | 删 peer：1:1 聊天清掉；仍属某频道的 peer 降级 `CHANNEL`（行保留供路由） |
| `unpairPeer(id)` | 置 `UNPAIRED`，共享密钥保留供重新配对 |
| `pairDevice(input)` / `cancelPairing(deviceId)` / `respondToPairing(input, accepted)` | LAN 配对（发起/取消/应答），走 `POST /nearby` 协议 |
| `channelSystemMessage(type, payload)` | 调试桩恒 false：该操作只经 `/peer_graphql` 入站，主 schema 实现会双重处理 |

`content` 是消息信封 JSON：`{"type":"TEXT|IMAGES|FILES|SHARE","value":{...}}`；
`data` 联合体由服务端解析（IMAGES/FILES 的 `ids` 是 URL-token 加密后的 `/fs` id）。

## 状态语义（ChatStatus）

- `SENT` 全部送达；`FAILED` 全部失败（或无可达 leader）；`PARTIAL` 部分失败；
  `PENDING` 已入库待投递。
- `statusData` JSON：`{"results":[{"peerId","peerName","error"}]}`；
  无 leader 时 `{"results":null}`。

## 配对协议（与 plain-app 逐字节对齐）

`POST /nearby`（LAN HTTPS，自签证书）承载三类消息前缀：
`PAIR_REQUEST:` / `PAIR_RESPONSE:` / `PAIR_CANCEL:`（另 `DISCOVER:` 存活探测恒 200）。
握手 = ECDH P-256 临时密钥交换（32 字节共享密钥即 XChaCha20 key）+
Ed25519 对规范化签名字符串签名（`fromIp` 不参与）+ 时间戳 ±5 分钟防重放。
成功后对端写进 `peers` 表（`PAIRED`）。

事件推送（WS，msg_type 沿用手机协议编号）见 `websocket.md` 的 chat 段。

## LAN 发现（mDNS）

NAS 在共享 mDNS responder 上广播 `_plainapp._tcp.local`（设备类型 `NAS`，
TXT 带 client id / 版本 / 平台，HTTPS 端口绑定后发布）——手机和桌面由此
发现本机并发起配对。常驻浏览器同时维护：

- **peer 在线状态**：`peers.online` 读发现快照（mDNS 见到即在线）；
- **peer 地址刷新**：已配对/已登录 peer 重新宣告时自动更新其 ip/port
  （换 IP 后下次投递即恢复）；
- **nearby 缓存**：`nearby_device_cache` 表持续记录见过的设备。

`updateDeviceName` 重命名会同步更新共享身份并重新发布 mDNS 实例。
投递失败触发 mDNS 重查询（`ChatHooks::rebrowse_peers`）刷新 peer 地址。

> 发现列表的 GraphQL 查询面（如 `nearbyDevices`）不在手机契约内，属
> 契约扩展决策——需要时单独评审（API_SPEC §8 流程）。
