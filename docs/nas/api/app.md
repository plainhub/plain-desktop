# App / Device / Update API

## Query

### `app: App!`

前端启动时调一次，拿到运行时配置。

```graphql
query {
  app {
    urlToken
    httpPort
    httpsPort
    appDir
    deviceName
    capabilities
  }
}
```

curl:

```bash
curl -s -X POST -H "Authorization: Bearer dev" \
  -H "Content-Type: application/json" \
  http://127.0.0.1:8080/graphql \
  -d '{"query":"{ app { httpPort httpsPort urlToken appDir deviceName capabilities } }"}'
```

响应：

```json
{
  "data": {
    "app": {
      "httpPort": 8080,
      "httpsPort": 8143,
      "urlToken": "abc123...",
      "appDir": "./tmp-data",
      "deviceName": "media box",
      "capabilities": ["MEDIA_TRASH", "MEDIA_SCAN", "DOC_PREVIEW"]
    }
  }
}
```

字段：

| 字段 | 类型 | 说明 |
|------|------|------|
| `urlToken` | `String!` | URL 分享 token（自动生成、持久化在 prefs） |
| `httpPort` | `Int!` | 配置文件里的 HTTP 端口 |
| `httpsPort` | `Int!` | 配置文件里的 HTTPS 端口 |
| `appDir` | `String!` | 应用数据目录（`PLAIN_NAS_DATA_DIR` env 或默认；fjall 库、prefs.json、日志都在这里） |
| `deviceName` | `String!` | 设备显示名（覆盖优先，空回退 hostname） |
| `capabilities` | `[Capability!]!` | 服务端声明的能力：`MEDIA_TRASH` 恒有；`MEDIA_SCAN` 恒有（内建扫描/索引引擎，web 首页 Files 卡片的扫描面板靠它显隐）；`DOC_PREVIEW` = 系统装了 libreoffice/soffice（office 预览可用）；`MIRROR_AUDIO` NAS 恒不声明 |

音频播放状态在独立的 `audioPlayback` 查询上（play mode + 当前曲，`currentPath` null = 空闲；
NAS 无服务端播放器，`isPlaying` 恒 false、`positionMs` 恒 0，音频在客户端渲染——见 `playlist.md`）；
媒体扫描进度是独立的顶层 `scanProgress` 查询（见 `scan.md`），都不在 `app` 下。

### `deviceInfo: DeviceInfo!` / `deviceStatus: DeviceStatus!`

**plain-app 契约形状**（`docs/device-info.md` 定稿：一套设备模型，两个面——
静态身份/规格取一次 `deviceInfo`，动态状态可轮询 `deviceStatus`；旧的 Go
plainnas 服务器监控形状与 `desktop` 子对象 / `battery` 查询全家已删除，
无兼容层）。

```graphql
{ deviceInfo {
  name platform manufacturer model     # model = DMI product_name（NanoPi R5S…）
  osName osVersion kernelVersion
  appVersion appBuildNumber language
  cpuArch cpuModel                     # cpuModel 可空顶层字段
  totalMemory totalStorage
  display { width height density }     # NAS 恒 null
  android { sdkVersion ... }           # NAS 恒 null（14 字段，无 buildHost/buildUser/serial/product）
}
deviceStatus {
  uptimeSec          # 秒（不再是 DeviceInfo.uptime 的毫秒）
  batteryLevel       # NAS 恒 null（无电池）
  charging           # NAS 恒 false
  temperatures { label celsius }   # /sys/class/thermal/thermal_zone*
  cpuUsage           # 0-100，两次 /proc/stat 采样差分（采样间隔 200ms）
  memoryAvailable    # /proc/meminfo MemAvailable
  storageAvailable   # 根文件系统可用
} }
```

NAS 取值：`platform = LINUX`；`manufacturer` = DMI `sys_vendor`；`name` =
设备显示名（`updateDeviceName` 覆盖值，空则回退 hostname，与 `App.deviceName`
同优先级）；`osName = "Linux"`、`osVersion` = `/etc/os-release` 的
`PRETTY_NAME`；`totalStorage`/`storageAvailable` = 根文件系统容量/可用。
`sims` 不在 NAS schema 中——NAS 无 SIM，web（nas 分支）文档已同步移除选择。

curl：

```bash
curl -s -X POST -H "Authorization: Bearer dev" \
  -H "Content-Type: application/json" \
  http://127.0.0.1:8080/graphql \
  -d '{"query":"{ deviceInfo { name platform osName cpuArch cpuModel totalMemory totalStorage } deviceStatus { uptimeSec cpuUsage memoryAvailable storageAvailable temperatures { label celsius } } }"}'
```

### `appUpdate: AppUpdate!`

检查 GitHub releases 是否有新版本。结果在进程内缓存 10 分钟。

```graphql
{ appUpdate { currentVersion latestVersion hasUpdate url } }
```

curl：

```bash
curl -s -X POST -H "Authorization: Bearer dev" \
  -H "Content-Type: application/json" \
  http://127.0.0.1:8080/graphql \
  -d '{"query":"{ appUpdate { currentVersion latestVersion hasUpdate url } }"}'
```

## Mutation

### `setHostname(name: String!): Boolean!`

修改主机名。**实际跑 `hostnamectl set-hostname <name>` + `systemctl try-restart avahi-daemon`**，需要 root 权限。原名 `setDeviceName`——与 plain-app 的 `updateDeviceName`（只改显示名偏好）太容易混淆，改名为 `setHostname` 把语义摆明。

```graphql
mutation { setHostname(name: "my-nas") }
```

host name 验证规则：
- lowercase only
- `[a-z0-9-]` only
- 不允许连续 `--`，不允许首尾 `-`
- 长度 1..=63
- `_` `.` ` ` 都转成 `-`

### `updateDeviceName(name: String!): Boolean!`

修改设备**显示名**（plain-app 同名 mutation 的语义）：只存显示名偏好，`app { deviceName }` 优先返回它；偏好为空时回退系统主机名。**不改 OS 主机名**（那是上面的 `setHostname`，需要 root）。首尾空白会被 trim，传空串即清除偏好、恢复显示主机名。web 设置弹窗的改名入口用的就是这个 mutation。

```graphql
mutation { updateDeviceName(name: "我的 NAS") }
```

### `setTempValue(key: String!, value: String!): TempValue!`

短期键值对（瞬时 UI 状态交接），内存实现不落盘，返回 `{key, value}` 方便前端回显。详见 `kv.md`。

```graphql
mutation { setTempValue(key: "upload-state", value: "active") { key value } }
```
