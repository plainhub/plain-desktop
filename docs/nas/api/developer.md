# Developer API（web UI `/developer/*` 子页面）

对齐 plain-app 契约（Kotlin `AppLogsGraphQL.kt` / `DataStoreGraphQL.kt` / `DbGraphQL.kt`），
服务 web UI 的四个 developer 子页面：`/developer/logs`、`/developer/datastore`、
`/developer/database`、`/developer/device-info`（deviceInfo 见 `app.md`）。

三端一致的存储拆分（plain-app：Jetpack DataStore + Room SQLite；plain-desktop：
`prefs.json` + `local_chat.db` + `local_library.db`；plain-nas：
**`prefs.json` + fjall（内部）+ `chat.db` + `library.db`**，后两者为 plain-rs 共享
SQLite，行为与桌面端一致）：

- **DataStore 页 = `<data_dir>/prefs.json`**：扁平 string→JSON map，存用户设置、
  设备身份与小体量应用状态（`device_name`、`client_id`、`url_token`、
  `signature_key_pair`、`password_hash`、`recent_files`、`samba_settings`
  等，键名对齐 plain-app 偏好；`favorite_folders` 与 `audio_play_mode` 已迁入
  `library.db`）。
  pretty-print 可手改，每次写入原子替换（tmp + rename），同值写入跳过落盘
  （`src/prefs.rs`）。
- **Database 页 = 两座 SQLite 用户库**：`chat.db`（chats/chat_channels/peers/
  nearby_device_cache/app_files/bookmarks/bookmark_groups）与 `library.db`
  （audio_queue_source/audio_queue_items/audio_playlists/audio_playlist_items/
  audio_play_history/tags/tag_relations/favorite_folders/library_prefs）。
  表名带库前缀：`chat.chats`、`library.tags`（分派在 `src/devtools_sqlite.rs`）；
  列元数据来自 `PRAGMA table_info`，行来自 `SELECT *` 的 JSON 字符串
  （数字保持数字、BLOB 渲染为 hex），`idKey` = 声明的主键列（复合主键取第一列）。
  浏览原语（标识符护栏、分页、删行）在 `plain_rs::sqlite_browse`，与
  plain-desktop 的 debug DB 页共享同一实现。
  内部 fjall 命名空间（event/media/session）是服务端内部状态，不出现在本页。
  偏好不出现在表里——各页各管各的，与手机/桌面一致。
- **日志**：`<data_dir>/logs/latest.log`，单文件、行式、新行在尾部；
  行格式 `YYYY-MM-DD HH:MM:SS.mmm LEVEL body`（UTC，详见 `src/log.rs`）。

## Query

### `appLogs(offset: Int!, limit: Int!, query: String!): [String!]!`

日志行，**最新在前**。`offset = 0, limit = 200` 即最新 200 行；倒序读按 64KB 块
从文件尾回放，内存只持有 `offset + limit` 行（plain-app `AppLogHelper` 语义：
跳过空行、剔除 `\r`、无结尾换行也能取到最后一行）。`query` 为共享搜索 DSL，
服务端只取其 `text:` 字段做大小写不敏感子串过滤（先于 offset/limit）；空串不过滤。

```bash
curl -s -X POST -H "Authorization: Bearer dev" -H "Content-Type: application/json" \
  http://127.0.0.1:8080/graphql \
  -d '{"query":"{ appLogs(offset: 0, limit: 50, query: \"\") }"}'
```

### `appLogPath: String!`

当前日志文件绝对路径（`<data_dir>/logs/latest.log`）。web UI 的下载按钮用它
拼 `/fs?id=<encrypted {path,name}>` 下载。

### `dbPath: String!`

两座 SQLite 库的绝对路径，逗号连接（`<data_dir>/chat.db, <data_dir>/library.db`）
——手机/桌面是单库单路径，NAS 是两座库。

### `dataStorePath: String!`

偏好文件绝对路径（`<data_dir>/prefs.json`）——与 `dbPath` 是两个真实文件，
对齐 plain-desktop 的 `prefs.json` / `local_chat.db` 二分。

### `dbTables: [String!]!`

两座 SQLite 库的全部用户表，按名排序，带库前缀（`chat.*` 块在 `library.*` 前）：
`chat.app_files, chat.bookmark_groups, chat.bookmarks, chat.chat_channels,
chat.chats, chat.nearby_device_cache, chat.peers, library.audio_play_history,
library.audio_playlist_items, library.audio_playlists, library.audio_queue_items,
library.audio_queue_source, library.favorite_folders, library.library_prefs,
library.tag_relations, library.tags`。SQLite 内部表（`sqlite_%`）不列出。

### `dbTableInfo(table: String!): DbTableInfo!`

```graphql
{ dbTableInfo(table: "library.tags") { idKey } }   # → {"idKey":"id"}
```

`idKey` = 声明的主键列（复合主键取第一列，如 `library.tag_relations` →
`tag_id`）。未知/无前缀/不安全的表名报错。

### `dbTableRowCount(table: String!): Int!`

表的行数（`SELECT COUNT(*)`）。

### `dbTableRows(table: String!, offset: Int!, limit: Int!): [String!]!`

表的分页行（`SELECT *`，rowid 自然序），每行一个 JSON 字符串：整数/浮点是 JSON
数字、文本是字符串、BLOB 是 hex 字符串、NULL 是 null；`limit` 上限 1000。

### `dataStoreEntries: [KeyValuePair!]!` `{ key value }`

prefs.json 的全部条目，键排序，value 以 JSON 文本渲染（字符串带引号、对象/数组
为紧凑 JSON——与 plain-desktop 的 `serde_json::Value::to_string()` 一致）。
读一个小文件，完全不碰 fjall。注意 `password_hash` 等敏感键也在页内——本页与
整库同权，仅限已认证管理员使用。

### `deviceStatus: DeviceStatus!`（见 `app.md`）

动态设备状态（电量/CPU/内存/温度/磁盘/运行时长），形状与取值详见
`app.md` 的 deviceInfo/deviceStatus 段。旧的 `battery` 查询全家
（`Battery` 类型 + 3 个枚举）已随该契约删除——无兼容层；`sims` 查询
不提供——NAS 无 SIM，web（nas 分支）的 `deviceInfoGQL` 已同步移除对它的选择。

## Mutation

### `clearAppLogs: Boolean!`

清空当前日志文件（truncate 为 0；活动 sink 句柄同步回卷，之后的日志继续追加）。

### `deleteDataStoreEntry(key: String!): Boolean!`

删除一条偏好（plain-app / plain-desktop 同语义：key 存在则删，不存在静默
返回 true）。行数据不经此接口（用 `deleteDbTableRows`）。

### `deleteDbTableRows(table: String!, ids: [String!]!): Boolean!`

按主键值批量删除某表的行。复合主键表按第一键列的值删（同值全删）。未知表或
空 `ids` 拒绝。

### 不实现：`createDbTableRow`

plain-app / plain-desktop 有该 mutation（往 SQLite 表插行），但没有任何 web 页面
调用；plain-nas 不提供（浏览原语 `plain_rs::sqlite_browse::insert_row` 已具备，
将来要接时直接在 mutation 层包一层即可）。

## 单测

- `tests/unit/prefs.rs` — 载入缺失文件、字符串/JSON 各形往返、同值跳写盘、
  remove、键序+JSON 渲染、重载一致、pretty 落盘、坏文件报错。
- `tests/unit/devtools_sqlite.rs` — 双库分派：`<db>.<table>` 命名、表排序、
  计数/行/列/主键路由到正确的库、复合主键取第一键列、未知/无前缀/注入式表名
  全拒、删除只影响指定库。
- plain-rs `tests/unit/sqlite_browse.rs` — 浏览原语本体：标识符护栏、表列表
  （排除 `sqlite_%`）、行 JSON 形状（数字/null/hex blob）、分页 + limit 1000
  钳制、PRAGMA 元数据、类型映射、删行/插行校验。
- `tests/unit/log.rs` — 文件 sink 只写已启用级别（副作用计数=文件行）、UTC
  时间戳形状、newest-first 读取（跨块/空行/CRLF/无尾换行/offset 窗口）、
  clear 后 sink 续写。
- `tests/unit/gql/{query,mutation}.rs` — schema 级端到端（`developer_*`、
  `device_info_serves_plain_app_contract`、`clear_app_logs_*`、
  `delete_data_store_entry_*`、`delete_db_table_rows_*`）。
- `tests/unit/gql/mod.rs::print_schema` — SDL 锁定 developer 面。
