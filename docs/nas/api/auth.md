# 鉴权 API

PlainNAS 支持两种模式：**Session 模式**（生产）和 **Dev 模式**（开发）。

## Dev 模式（推荐用于测试）

config.toml 里：

```toml
[auth]
dev_token = "dev"   # 默认
```

直接 `Authorization: Bearer dev` 即可。**所有本文档的 curl 示例都走 dev 模式**。

```bash
curl -s -X POST -H "Authorization: Bearer dev" \
  -H "Content-Type: application/json" \
  http://127.0.0.1:8080/graphql \
  -d '{"query":"{ app { httpPort httpsPort } }"}'
```

dev 模式下：
- `client_id` 固定为 `"dev"`
- 不需要 login
- body 是**明文 JSON**（不加密）

## Session 模式（生产）

### `POST /init`

plain-desktop（前端）的**唯一免登录 API**，用于登录前探测。契约与 plain-app 对齐
（plain-app `SystemRoutes.kt` `post("/init")`）：**本接口不需要鉴权，任何情况下都不返回 401**。

- header：`c-id: <client_id>` **必填**，缺失返回 `400`（与 plain-app 一致）。
- body：忽略（plain-app 用 body 做自动登录探测；对已设密码的 NAS 来说探测结果不影响应答，plain-nas 不处理）。

响应永远是 `200`：

```json
{"needsSetup": true|false, "signaturePublicKey": "<base64_ed25519_pub_32B>"}
```

- `needsSetup: true`：password 未设置，客户端进 setup 流程。
  （plain-nas 扩展；plain-app 没有 setup 概念，未设密码时直接把生成的 password 放进响应交给桌面端。）
- `needsSetup: false`：password 已设置。客户端自行分流——本地存有 token 就乐观进入自动登录
  （会话真失效时由后续 API 的 401 踢回登录页，与 plain-app 生态一致），否则显示登录表单。

`signaturePublicKey` 是服务端 Ed25519 公钥（标准 base64，32 字节原始值），首次启动生成后存 KV store
（prefs.json 偏好 `signature_key_pair`，base64 的 64B keypair，同 plain-desktop 存法），重启不变；客户端用它对签名登录响应做 TOFU 校验。

> plain-desktop 的登录走 `/ws` WebSocket 握手（`auth=1`，Ed25519 签名 + ECDH 派生 token），
> plain-nas 已实现；`POST /auth` 仍是普通 REST 登录入口（plain-app 系客户端使用）。

### `POST /auth/status`

检查 admin password 是否已设置。

```bash
# Response (password not set yet):
{"needsSetup": true}

# Response (password already set):
{"needsSetup": false}
```

实际实现里 PlainNAS **强制要求 password 先设**——`/auth` 在没 password 时返回 409。

### `POST /auth/setup`

**只在 password 未设时**调用一次。用 admin password 初始化。

```bash
# body: ChaCha20-Poly1305 加密的 JSON
# plaintext: {"password": "<sha512_hex_of_password>"}
# header: c-id: <new_client_id>
# header: Content-Type: application/octet-stream
```

返回：

```json
{"nasId": "...", "token": "<base64_xchacha20_key>"}
```

`token` 是 32 字节 XChaCha20 key 的 base64。前端存到 `localStorage.auth_token`，并把 `client_id` 存到 `localStorage.client_id`。

### `POST /auth`

登录。body 是 ChaCha20-Poly1305 加密的 JSON：

```json
{
  "password": "<sha512_hex_of_admin_pwd>",
  "browserName": "...",
  "browserVersion": "...",
  "osName": "...",
  "osVersion": "...",
  "isMobile": false
}
```

header：
- `c-id: <client_id>`（前端随机生成的 22 字符 base64url）
- `Content-Type: application/octet-stream`

响应（同 setup）：

```json
{"nasId": "...", "token": "<base64_xchacha20_key>"}
```

失败：

| HTTP | body | 含义 |
|------|------|------|
| 401 | `{"errors":[{"message":"Unauthorized"}]}` | password 不对 / 解密失败 |
| 400 | `{"errors":[{"message":"Bad request"}]}` | body 是空 / JSON parse 失败 |
| 400 | `{"errors":[{"message":"Missing client id"}]}` | 没 `c-id` header |
| 409 | `{"errors":[{"message":"Password not configured"}]}` | 还没调过 `/auth/setup` |

### 调用 GraphQL（session 模式）

```bash
# body: ChaCha20-Poly1305 加密的 GraphQL JSON 请求
# key: <token from /auth response, base64 decoded>
# header: c-id: <client_id>
# header: Content-Type: application/octet-stream
# 响应: 加密的 JSON，要用同一个 key 解密
```

完整 e2e 流程见 `websocket.md` 里的"session 模式握手示例"。

## 错误码

| 错误 | 含义 |
|------|------|
| `password_not_configured` | 还没 setup |
| `decrypt_failed` | 加密 body 没法用 session key 解密（key 不对 / body 损坏） |
| `bad_password` | 解密成功但 password hash 不匹配 |
| `missing_client_id` | `c-id` header 是空 |
| `bad_request` | body 是空 / JSON parse 失败 |
