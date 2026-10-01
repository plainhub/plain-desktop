# 存储 API

对齐 Go 版 `internal/graph/storage*.go`：`mounts` 返回挂载卷 + 未挂载分区，
`disks` 返回底层整盘，`formatDisk` 抹盘重建单分区 ext4 并自动挂载。

## Query

### `mounts: [StorageMount!]!`

挂载卷（`/proc/mounts` + `lsblk` 元数据）+ 未挂载分区（`lsblk`）。前端用
`diskId` 把卷关到磁盘，用 `path` 区分卷（空）与分区（块设备路径）。

```graphql
{ mounts {
  id name alias label mountPoint fsType
  totalBytes usedBytes freeBytes
  remote driveType
  diskID path partitionNum uuid
} }
```

> SDL 字段名是 `diskId`（async-graphql 默认 camelCase），Rust 端 `disk_id`。

curl：

```bash
curl -s -X POST -H "Authorization: Bearer dev" \
  -H "Content-Type: application/json" \
  http://127.0.0.1:8080/graphql \
  -d '{"query":"{ mounts { id name mountPoint fsType totalBytes usedBytes freeBytes diskId path partitionNum } }"}'
```

卷的稳定 ID（`fsuuid:<uuid>` / `dev:<path>` / `remote:<src>`）与分区 ID
（`part:<uuid>` 或 `part:<path>`）同 Go 版。卷字段：

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | `ID!` | 见上 |
| `name` | `String!` | 卷名（挂载点 basename / 分区设备名） |
| `alias` | `String!` | 用户用 `setMountAlias` 设的别名（空串 = 未设） |
| `label` | `String` | 分区 label（lsblk） |
| `mountPoint` | `String!` | 挂载路径（分区可为空串） |
| `fsType` | `String!` | 文件系统类型（ext4 / btrfs / ntfs / vfat / ...） |
| `totalBytes` | `Long!` | 总容量（分区为裸分区大小） |
| `usedBytes` | `Long!` | 已用（`statfs`） |
| `freeBytes` | `Long!` | 可用（`statfs`） |
| `remote` | `Boolean!` | NFS / CIFS / SSHFS 等网络挂载 |
| `driveType` | `DriveType!` | plain-app 契约枚举；NAS 恒 `INTERNAL_STORAGE` |
| `diskId` | `String!` | 所属磁盘 stable ID（`disk:` / `diskbyid:`，同 `disks.id`） |
| `path` | `String` | 块设备路径（分区才有：`/dev/sdb1`） |
| `partitionNum` | `Int` | 分区号 |
| `uuid` | `String` | 分区/filesystem UUID |

过滤规则（同 Go）：跳过伪文件系统（proc/sysfs/tmpfs/overlay/...）、
`/boot`/`/boot/efi`/`/efi`/`/boot/firmware`、bind mount、zram/loop、
非块非远程来源；LVM2_member 分区、无文件系统的 <32MB 小分区隐藏。

### `disks: [Disk!]!`

底层块设备（整盘，`lsblk` TYPE=disk；隐藏 zram/loop/ram）。

```graphql
{ disks { id name path sizeBytes removable model } }
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | `ID!` | `diskbyid:<by-id 链接名>`（优先，wwn > nvme > ata > scsi > usb）或 `disk:<内核名>` |
| `name` | `String!` | 内核设备名（`sda` / `nvme0n1`） |
| `path` | `String!` | `/dev/sda` |
| `sizeBytes` | `Long!` | 容量（字节） |
| `removable` | `Boolean!` | lsblk RM（缺失时 sysfs 兜底） |
| `model` | `String` | 型号（sysfs 优先于 lsblk 列） |

## Mutation

### `formatDisk(path: String!): Boolean!`

格式化一块磁盘为单分区 ext4（wipefs 清签名 → sfdisk 建 GPT 单分区 →
mkfs.ext4 -L plainnas）。**会先 umount，破坏性不可逆**。

```graphql
mutation { formatDisk(path: "/dev/sdb") }
```

安全检查（同 Go）：拒绝非 `/dev/` 路径、非整盘设备、挂了 `/` 的系统盘；
先卸载盘上所有挂载点并复核。执行期间 automount 协调器被抑制，完成后
触发一次协调，把新文件系统按持久化的 fsuuid→slot 映射挂到 `/mnt/usbX`。

错误：

| 错误 | 含义 |
|------|------|
| `format_disk_failed` | 任意步骤失败（wipefs / sfdisk / mkfs / mount） |
| `format_disk_failed: <path>: <err>` | 详细错误（来自 stderr） |

成功会写一条 `format_disk` audit event；卸载的每个挂载点各写一条
`unmount` event。

### `setMountAlias(id: ID!, alias: String!): Boolean!`

给某个 mount 设用户自定义别名（存 prefs 的 `volume_alias` map）。

```graphql
mutation { setMountAlias(id: "fsuuid:abc-123", alias: "My Photos") }
```

## 自动挂载（automount）

同 Go 版 `EnsureMountedUSBVolumes`：启动时扫描所有带 UUID 的文件系统，
把未挂载的挂到 `/mnt/usbX`；slot 分配持久化（`fsuuid_slot_map`），拔盘
期间 slot 保留不被抢占。热插拔经 `udevadm monitor` 事件驱动（700ms 防抖）
触发协调；每次成功挂载写 `mount` event，失败写 `mount_failed` event。
