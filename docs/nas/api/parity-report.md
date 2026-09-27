# GraphQL API 对齐报告（Rust plainnas ↔ Go plainnas ↔ plain-app）

> 2026-09-17。对照物：Go 版 `~/Projects/smartcoding/plainnas`（`internal/graph/schema.graphql`，442 行）与
> plain-app 共享 schema（`plain-app/shared/apitest/schema.graphqls`，手机全集）。
> 验收：`scripts/api-tests/gql.sh` **91 passed / 0 failed**（2 项 root-only 按设计 SKIP）。

## 总览

- Go schema 声明 **29 个 Query + 46 个 Mutation**；Rust 端字段面已 1:1 全部存在，无缺失。
- Rust 额外暴露的 NAS 扩展（Go 没有）：Query `trashItems`、`deleteTrashItem`、`scanProgress`（web 垃圾桶页/索引卡片在用）；Query `audioPlaylist`（web 播放器启动加载，plain-app 形状）；Query `bookmarks`/`bookmarkGroups` + 7 个书签 Mutation（plain-app 书签面全量，web 书签页在用，见 `docs/api/bookmarks.md`）。均不影响 Go 客户端。
- 本轮修复（此前 gql.sh 70/77）：`App.scanProgress` 缺失、`files.root` 必填、`fileInfo.id` 必填、媒体列表 `sortBy` 必填、`File.childCount` 缺失；并把 `trashMediaItems/restoreMediaItems/deleteMediaItems` 从回显 stub 实现为真实文件操作。

## 与 plain-app 的形状差异（逐项）

### 已消解的差异（本轮处理）

| API | plain-app/Go 形状 | Rust 原状 | 处理 |
|---|---|---|---|
| `files` | Go 无 `root` 参数（root 走 DSL）；web 端发 `root` | `root` 必填 | 改为**可选**（缺省走 DSL `root_path`→`/`），两端兼容 |
| `files` 子计数 | plain-app 叫 `children`，Go 叫 `childCount` | 只有 `children` | **双字段同值**，两方言均可 |
| `fileInfo(id)` | Go 必填 `id`（前端没有 id） | 必填 | 改为可选（忽略） |
| `images/videos/audios(sortBy)` | Go 必填 | 必填 | 可选，缺省 `DATE_DESC` |
| `App.scanProgress` | Go 有；web 首页索引用 | 缺失 | 补上（indexed/pending/total/state） |
| `Image/Video.takenAt` | web fragment 请求；Go NAS 没有 | 缺失（整查询报错） | 补上；**暂回文件 mtime，EXIF 未实现** |
| `Image/Video/Audio.title` | — | 图片/视频为空串 | 空时回退文件名 |
| `trashMediaItems` 等三个 | Go 真实执行（ids:/text DSL，≤10000 条） | 回显 stub | 实现真实 trash/restore/delete，逐条错误按 Go 吞掉 |

### 保留的既有差异（有意或低风险）

| 差异 | 说明 | 风险 |
|---|---|---|
| `sortBy` 在 Rust schema 中可空 | Go 为必填。发 sortBy 的客户端不受影响 | 低 |
| `files.root` 参数 | Go schema 没有；web 在用。可选参数，Go 客户端不发即无感 | 低 |
| 时间戳类型 | plain-app 用 `Instant`、Go 用 `Time` 自定义标量，Rust 一律 `String`（RFC3339, UTC） | 无（线上都是 JSON 字符串） |
| `Long` vs `Int` | Go `Long` 自定义标量 / plain-app `Long`；Rust i64 输出为 `Int`。线上同为 JSON 数字 | 无 |
| `Audio.albumFileId` | 恒为 `""`（专辑封面关联，未实现封面映射） | 低：UI 显示占位封面 |
| `Audio.duration` | **已解决（2026-09-19）**：读路径惰性探测 + 持久化（`media_scan::hydrate_metadata` / `hydrate_search_page`，Go media_helper 同构）；MP4 族容器级 moov→mvhd 解析（`media::metadata::mp4_duration_secs`，lofty 拒收无音轨 mp4），音频走 lofty。探测失败不盖 ref 戳、下次读重试 | 无 |
| `mediaBuckets.topItems` | 返回桶目录内前 4 个**同类型**文件路径（Go 语义一致；顺序为 path 索引序，非时间序） | 低 |
| `imageSearchStatus` | 手机端 AI 搜图状态（CLIP 模型下载/索引进度），Go NAS 没有。**已解决（2026-09-17，双层）**：Rust NAS 返回恒 `UNAVAILABLE` 的规范 stub（旧客户端探测不再报 Unknown field）；web 端 `useImageSearchStatus` 以 `enabled` 门控（设备类型已知且非 NAS 才发查询），`ImageSearchButton` 在 NAS 隐藏（同 FDROID 渠道处理） | 无 |
| `docs`/`docCount` 页面 | web 有 docs 页（`docs`/`docCount`/`docExtGroups`），Go NAS 与 Rust NAS 后端均无此查询。**已解决（2026-09-21）**：Rust NAS 实现文档库——`infer_type` 经共享 MIME 表（plain-rs，补 doc/docx/xlsx）分类 `doc` 类型，tantivy 索引新增 `ext` 字段（`ext:` 过滤 + docExtGroups 词典聚合），新增 `docs`/`docCount`/`docExtGroups` 查询与 `Doc`/`DocExtGroup` 类型；`mediaBuckets(DOC)` 与 trash/restore/delete/move 四个 media action 放开 DOC。web 端已在 capability 重构中全量接好 docs（无门控），后端补齐即通 | 无 |
| `moveMediaItems` | 手机端「移动到文件夹」mutation，Go NAS 与 Rust NAS 均无。**已解决（2026-09-17）**：images/videos/audios 三页移动按钮加 `!isNas` 门控 | 无：NAS 移动媒体为后续后端功能项 |
| `updateDeviceName` | 手机端改设备显示名。**已解决（2026-09-18，对齐 plain-app 语义）**：Rust NAS 实现 `updateDeviceName`（存显示名偏好，`app.deviceName` 优先返回、空回退 hostname，不改 OS 主机名）；web 设置弹窗重命名入口对 NAS 恢复可见。`setDeviceName`（root，改 hostname）保留不变 | 无 |

## 超出 Go 的行为（Rust 扩展，均为 web 需要或修复）

1. `trashItems` / `deleteTrashItem` / `scanProgress`（Query）——垃圾桶页与索引卡片。（2026-09-23 更名：原 `listTrash` / `deleteTrash`。）
2. `audioPlaylist(offset, limit)`（Query）——web 播放器启动加载队列；数据与 `App.audios` 同源（`audio_playlist` KV），Go 只暴露 `App.audios`。
3. 书签全家桶（plain-app 面）：`bookmarks`/`bookmarkGroups` Query + addBookmarks/updateBookmark/deleteBookmarks/recordBookmarkClick/createBookmarkGroup/updateBookmarkGroup/deleteBookmarkGroup，存储 `bookmark:{id}`/`bookmark_group:{id}`；deleteBookmarkGroup 把成员移入未分组（同手机端）。**title/favicon 页面抓取未实现**（title 初始 = url，faviconPath 恒 ""，可手动改名）。
4. `media:type:` 索引砍除、fid 索引回收、bucket 计数器、并行批量扫描引擎（性能 2.4×，见 SHORT_TERM）。
5. `File.permission`/`mediaId` 等恒空字段保留是为 plain-app schema 形状一致。

## 已知问题 / 待办

1. **`deviceInfo` 在 Mac 上全 0**（os/cpuCores/memoryTotalBytes…）：`device_info.rs` 读 `/proc`，Linux-only。smartbox（Linux）正常。如需 Mac 开发体验，可加 sysctl 分支。
2. **媒体 trash/restore 的真实文件流无法在 Mac 非 root 验证**：`.nas-trash` 建在挂载点根（`/`），非 root EPERM；与 Go 相同按条目吞错，GraphQL 仍返回成功。待 smartbox 上人工点一遍（含 `trashFiles` 根目录写权限）。
3. **删除/恢复后 tag 关系清理**：trash 时按 uuid+path 双键清理（Go 只按 uuid）；restore 不自动恢复 tag 关系（Go 同）。注意 Rust 的 tag key 历史上混用 path/uuid（`file_info` 用 path 查），plain-app 端用文件 id——键口径统一是后续清理项。
4. **NAME 排序上限 10 万条**：媒体库超 10 万时 NAME_*/仅返回前 10 万内的排序窗口（DATE/SIZE 排序无限制）。
5. **`excluded_dir` 是查询时过滤**（Go 同款）：设置变更即时生效，但 `.nas-trash` 内 trash 行仍可被 `trash:true` 搜到，属预期。
6. **`playAudio` 未在 gql.sh 覆盖**，Rust 已实现（playlist 模块）；建议补一条测试。
7. **`setDeviceName`/`trashFiles`** 需 root（systemd 部署下可用），gql.sh 已按环境 SKIP。

## 复验方式

```bash
cargo build
mkdir -p tmp-e2e/data
openssl req -x509 -newkey rsa:2048 -keyout tmp-e2e/key.pem -out tmp-e2e/cert.pem -days 30 -nodes -subj "/CN=localhost"
printf '[server]\nhttp_port = 8090\nhttps_port = 9443\n\n[auth]\ndev_token = "dev"\n' > tmp-e2e/config.toml
PLAIN_NAS_TLS_CERT=./tmp-e2e/cert.pem PLAIN_NAS_TLS_KEY=./tmp-e2e/key.pem \
PLAIN_NAS_ALLOW_NONROOT=1 PLAIN_NAS_DATA_DIR=./tmp-e2e/data \
PLAIN_NAS_CONFIG=./tmp-e2e/config.toml ./target/debug/plainnas run &
HOST=127.0.0.1:8090 ./scripts/api-tests/gql.sh   # 期望 91 passed, 0 failed
```

> dev token 走 config `[auth] dev_token`，空数据目录冷启动时 `Bearer dev` 不是默认可用的（必须配置 dev_token）。

回归测试：`cargo test`（195 通过；`mountinfo::resolve_root` 为既有 Linux-only 项）。
