# Samba API

完整对齐 Go 版（`internal/samba/samba.go` + `internal/graph/samba_api.go`）：
设置存 prefs，应用时渲染 `/etc/samba/smb.conf`、确保 `nas` unix 用户、
用 `smbpasswd` 设置密码、enable + restart systemd 单元（`smbd` / `samba` /
`smb`，取第一个 loaded 的）。

## 模型

```graphql
enum SambaShareAuth { Guest Password }

input SambaShareInput {
  name: String!
  sharePath: String!    # 注意 sharePath 不是 share_path
  auth: SambaShareAuth!
  readOnly: Boolean!
}

type SambaShare {
  name: String!
  sharePath: String!
  auth: SambaShareAuth!
  readOnly: Boolean!
}

type SambaSettings {
  enabled: Boolean!
  username: String!     # 恒 "nas"
  hasPassword: Boolean!
  shares: [SambaShare!]!
  serviceName: String!
  serviceActive: Boolean!
  serviceEnabled: Boolean!
}
```

## Query

### `sambaSettings: SambaSettings!`

设置来自 prefs；`serviceName` / `serviceActive` / `serviceEnabled` 是
`systemctl show <unit>` 的实时状态（无 systemd 时退回持久化值）。

```graphql
{ sambaSettings {
  enabled username hasPassword
  shares { name sharePath auth readOnly }
  serviceName serviceActive serviceEnabled
} }
```

curl：

```bash
curl -s -X POST -H "Authorization: Bearer dev" \
  -H "Content-Type: application/json" \
  http://127.0.0.1:8080/graphql \
  -d '{"query":"{ sambaSettings { enabled shares { name sharePath } serviceActive } }"}'
```

## Mutation

### `setSambaSettings(input: SambaSettingsInput!): Boolean!`

```graphql
mutation { setSambaSettings(input: {
  enabled: true
  shares: [
    { name: "photos", sharePath: "/mnt/photos", auth: Guest, readOnly: true }
    { name: "private", sharePath: "/mnt/private", auth: Password, readOnly: false }
  ]
}) }
```

语义（同 Go）：

1. 校验：启用时至少一个 share；有 Password share 但从未设过密码 →
   `password required`（先调 `setSambaUserPassword`）。
2. 持久化到 prefs。
3. 应用到系统：
   - share 路径不存在则创建（0755），不是目录则报错；
   - 确保 `nas` 系统用户（`useradd -M -r -s /usr/sbin/nologin nas`，Alpine
     走 `adduser -D -H`）；
   - 渲染 smb.conf（见下）；
   - enable + `systemctl restart <unit>`；关闭时 stop + disable。

smb.conf 的 macOS 兼容：检测 `fruit` / `catia` / `streams_xattr` VFS 模块
（`smbd -b` 的 MODULESDIR + 常见发行版路径），存在才启用（`fruit:aapl`、
`fruit:posix_rename` 等）；share 目录支持 xattr 时用 `streams_xattr`，
否则回退 AppleDouble sidecar（`fruit:metadata = netatalk`）。share 名清洗
为 `[A-Za-z0-9._-]`（空格→下划线，≤32 字符），重名自动加 `-2` 后缀；
guest share `guest ok/guest only`，password share `valid users = nas`。

### `setSambaUserPassword(password: String!): Boolean!`

设置 samba 用户的密码（`smbpasswd -a -s nas`，stdin 喂两遍密码），
成功后持久化 `hasPassword`；samba 启用中则重新 apply。

```graphql
mutation { setSambaUserPassword(password: "secret") }
```

**注意**：`password` 是明文（GraphQL 路径要求 session 模式加密 body）。
