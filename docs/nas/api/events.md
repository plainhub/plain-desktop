# Session / Event API

## 模型

```graphql
type Session {
  clientId: String!
  clientName: String!
  lastActive: Instant!
  createdAt: Instant!
  updatedAt: Instant!
}

enum AuditEventType {
  LOGIN
  LOGIN_FAILED
  LOGOUT
  REVOKE
  SET_HOSTNAME        # setHostname（改 OS hostname）
  UPDATE_DEVICE_NAME  # updateDeviceName（改显示名偏好）
  MOUNT
  MOUNT_FAILED
  UNMOUNT
  FORMAT_DISK
  FORMAT_DISK_FAILED
}

type AuditEvent {
  id: ID!
  type: AuditEventType!
  message: String!
  clientId: String!
  createdAt: Instant!
}
```

`Instant` 是 RFC3339 UTC 字符串（API_SPEC §1）。审计 kind 在 KV 里仍存 snake_case
字符串（`"login"`、`"format_disk"`、…）；`EventType` 只是 GraphQL 出口的枚举视图，
未知 kind 的旧行会被 resolver 跳过（不做存量迁移）。

## Query

### `sessions: [Session!]!`

```graphql
{ sessions { clientId clientName lastActive createdAt } }
```

curl：

```bash
curl -s -X POST -H "Authorization: Bearer dev" \
  -H "Content-Type: application/json" \
  http://127.0.0.1:8080/graphql \
  -d '{"query":"{ sessions { clientId clientName } }"}'
```

### `auditEvents(offset: Int!, limit: Int!, query: String!): [AuditEvent!]!`

最新在前。`query` 是共享 DSL（API_SPEC §5），只有 `text:` 字段生效——对
`type + message` 做大小写不敏感子串过滤，先过滤后分页。

```graphql
{ auditEvents(offset: 0, limit: 100, query: "text:login") {
  id type message clientId createdAt
} }
```

## Mutation

### `logout: Boolean!`

退出当前 session。写 `logout` audit event。

```graphql
mutation { logout }
```

### `revokeSession(clientId: String!): Boolean!`

强制让另一个 client 下线（管理员功能）。

```graphql
mutation { revokeSession(clientId: "abc123") }
```
