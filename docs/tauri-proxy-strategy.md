# Tauri Proxy Strategy (Performance & Stability)

桌面 webview 的唯一网络传输：常驻本地反向代理 `src-tauri/src/http_proxy/`。
webview 里只有普通的 `fetch` 和 `new WebSocket`，与 Web 构建完全同构；
区别只是 Tauri 构建里 API 地址被重写到电脑上的回环地址。

```
webview ──plain HTTP/WS──► 127.0.0.1:N（http_proxy）──TLS（自签证书可接受）──► 设备
webview ──────────plain HTTP/WS 直连─────────► localhost:PORT（桌面本地服务，本地模式）
```

## 目标（2026-09-26 用户指令）

- 干掉 `ipc://` 上的 HTTP / WebSocket 调用（`http_request`、`ws_start_proxy`
  两条 Tauri 命令已删除，`commands/http_client.rs`、`commands/ws_proxy.rs`
  文件已删除）。
- Tauri App 跟 Web 一样是正常 HTTP/WebSocket 调用，API 地址来自电脑；
  web 代码里不再有「换传输实现」的 `__IS_TAURI__` 分支，只剩 URL 选择。
- API 结构与形状跟 plain-app 一致（`/graphql`、`/upload`、`/upload_chunk`、
  `/fs`、`/peer_graphql`、WS 事件面均复刻 plain-app 契约）。

## 传输选择（`src/lib/api/http.ts` + `src/lib/api/api.ts`）

| 调用 | Web 构建 | Tauri 本地模式 | Tauri 远程设备 |
|---|---|---|---|
| `httpRequest(url)` | `fetch(url)` | `fetch(url)`（http://localhost:PORT 直连） | `fetch(proxyHttpUrl(url))` → `http://127.0.0.1:N<path>?_pt=<https设备base>` |
| `openSocket(url, cid)` | `new WebSocket(url)` | `new WebSocket(url)`（ws://localhost:PORT 直连） | `new WebSocket(buildProxyWsUrl(...))` → `ws://127.0.0.1:N<path>?<原查询>&_pt=<ws(s)设备base>&_cid=<peer id>` |
| `<img>/<video>/XHR 上传 | 直连设备 URL | 直连 | `proxyUrlFor(base, path)`（`_pt` 重写，同上） |

规则集中在 `api.ts`：`proxyHttpUrl`（https→代理）、`proxyWsUrlFor`（非回环
ws/wss→代理，回环直连）、`proxyUrlFor`（浏览器发起的 URL）。纯函数
`buildProxyHttpUrl` / `buildProxyWsUrl` 不含平台判断，由
`tests/lib/api/api.test.ts` 锁死。

## 代理的 WebSocket 中继

`http_proxy/mod.rs::relay_websocket`：

1. 收到 `GET + upgrade: websocket`，从 `_pt` 取目标 base、`_cid` 取 peer id。
2. `_cid` 非空时经 `PeerResolver`（`NearbyDiscoverManager::peer_address`，
   mDNS 保活的 peers 表）把目标 authority 换成 peer 当前 `ip:port`——
   设备换 IP 后重连自动走新地址。
3. `wss→https`、`ws→http` 后用共享 reqwest client 发起 upgrade 请求；
   客户端的 `sec-websocket-key` 原样转发，设备的 `101` 头原样回给 webview
   （accept 校验端到端成立）。
4. 之后 `copy_bidirectional` 字节级对拷——不解析帧，ping/close/扩展全部
   端到端透传。非 101 的拒绝按普通响应转发。

被删除的旧机制（不再存在）：`invoke('http_request')` IPC fetch（2 字节
status 前缀协议）、`invoke('ws_start_proxy')` 每连接一个临时 TCP 中继、
`invoke('peer_address')` 前端地址重解析、`TauriWebSocket`/`tauriFetch`
两个 webview 侧替身类。

## 错误路径与控制台噪音（2026-09-26）

- **HTTP 错误响应带 CORS**：代理的 400/502 都注入 `access-control-allow-*`，浏览器如实
  报「502 Bad Gateway」而不是误导性的「Origin is not allowed by Access-Control-Allow-Origin」。
  设备不可达时每分钟每个 peer 的轮询失败仍会在 console 显示一行真实状态——那是诚实信号。
- **WS 上游失败合成 101 + close(1011)**：拨号失败/设备拒绝 upgrade 时，代理用客户端的
  key 算出合法 `sec-websocket-accept` 完成握手（SHA-1 手写在 `http_proxy/utils.rs`，
  RFC 6455 向量锁死），随后发送 close 帧 1011。浏览器视为干净关闭——不刷握手错误；
  App 通过既有 `onclose` + 退避重连感知失败。
- **GraphQL 调用日志**（dev）：`gql-client.ts` 每条调用打印
  `[gql] → opName @host {query,variables}`（加密前全文）/ `[gql] ← opName (耗时·enc·dec) 响应全文`
  / 失败时 `[gql] ✗ opName 原因`（warn）。guest 分享页同款。开关
  `window.__PLAIN_LOG__`，`import.meta.env.DEV` 自动开启（main.ts）。

## Why This Is Fastest

- **连接复用**：代理对 keep-alive 安全的响应（有 content-length、无
  content-encoding）复用回环 TCP，且共享 reqwest client 复用设备 TLS
  session——视频 seek 的高 RTT 场景无重复握手。
- **流式转发**：响应逐 chunk 转发从不整体缓冲（视频/下载）；WS 是字节级
  对拷，无帧解析开销（1080p60 镜像流 ~6MB/s 直接走 TCP）。
- **无 IPC 序列化**：所有数据面走 TCP，`ipc://` 上只剩窗口/偏好/采集等
  shell 命令。

## Stability Notes

- 自签证书：reqwest `danger_accept_invalid_certs(true)`，webview 永远不直接
  碰设备 TLS。
- 代理剥离 `origin` 头并注入 `access-control-allow-*`（preflight OPTIONS
  本地应答，`allow-headers: *`）——fetch/XHR/WebSocket 都能过。
- WS 中继按连接隔离：每条 WS 独立 spawn，任一断开不影响其他。
- 端口：启动时 `127.0.0.1:0` 拿临时端口，经 `http_proxy_port` 命令一次性
  告知前端（shell 级 bootstrap，非数据通道）。

## Scope

适用于本仓库 Tauri 桌面模式的全部数据面传输。剩余 `__IS_TAURI__` 判断仅属
两类：`api.ts` 的 URL 选择（7 处）与桌面 shell（标题栏/菜单/窗口/采集/
prefs），web 代码不再按平台切换传输实现。
