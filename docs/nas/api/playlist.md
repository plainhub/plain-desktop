# 音频播放 API（plain-app `AudioGraphQL` 契约）

> 2026-09-19 起整体复刻 plain-app：播放队列 = **source（用户播放列表或整个曲库）+
> 手动队列（"下一首播放"/"加入队列"）+ 当前曲**，队列永不物化，next/prev 按需从
> source 解析（1 万首曲库与 1 首成本相同）。旧版"整个队列一个 JSON blob"
> （`audio_playlist` 键 + `audioPlaylist` query + `App.audios`）已移除，首次访问时
> 旧 blob 一次性迁移进手动队列（对齐 plain-app `ensureMigrated`）。

## 播放顺序

```
source[0 .. currentPos]  →  手动队列  →  source[currentPos+1 ..]
```

当前曲之后的头段、手动项、再接 source 余下部分。手动入队的曲目若同时存在于
source，其 source 副本视为 **superseded**——渲染/计数/顺序跳过时都不出现，
队列因此永不重复。

## 模型

```graphql
type PlaylistAudio { title: String! artist: String! path: String! durationMs: Long! }  # 毫秒

type AudioPlaylist {   # 用户播放列表
  id: ID! name: String! itemCount: Int!
  createdAt: Instant! updatedAt: Instant!
}

type AudioPlayHistory { # 最近播放（保留最新 200 条，超过 250 时裁剪）
  path: String! title: String! artist: String!
  durationMs: Long! playCount: Long! playedAt: Instant!
}

enum MediaPlayMode { REPEAT REPEAT_ONE SHUFFLE }
```

`App.audioMode: MediaPlayMode!`（枚举）、`App.audioCurrent: String!`（当前曲 path，
服务端是播放状态机，resolve 即镜像）。

## Query

### `audioQueueItems(offset: Int!, limit: Int!, query: String!): [AudioItem!]!`

当前播放队列分页（手动项 + source 上下文），永不物化整个队列。`query` 为共享
搜索 DSL，服务端只取其 `text:` 字段做大小写不敏感子串过滤（title/artist/path），
过滤先于分页；空串不过滤。
web 播放器启动加载：`items: audioQueueItems(...) + total: audioQueueItemCount`。

### `audioQueueItemCount: Int!`

队列总长（superseded 的 source 副本不计入）。

### `audioLyrics(path: String!): String!`

抽取音频内嵌歌词，1:1 移植 plain-app `EmbeddedLyrics`：ID3v2 USLT/ULT（mp3，
v2.2/2.3/2.4、unsync 标志、UTF-16/Latin-1/UTF-8）、Vorbis 注释
`LYRICS`/`UNSYNCEDLYRICS`（flac）、MP4 `©lyr` atom（m4a）。无歌词/不认识的
容器返回空串。

### `audioPlaylists: [AudioPlaylist!]!`

用户播放列表，按 `updatedAt` 倒序，`itemCount` 为实时条数。

### `audioPlaylistItems(id: ID!, offset: Int!, limit: Int!, query: String!): [AudioItem!]!`

单个播放列表的曲目，position 顺序分页。`query` 的 `text:` 字段过滤规则同
`audioQueueItems`。

### `audioPlaylistItemCount(id: String!): Int!`

### `audioPlayHistory(limit: Int!, offset: Int!, query: String!): [AudioPlayHistory!]!`

最近播放，`playedAt` 倒序。`query` 的 `text:` 字段过滤规则同 `audioQueueItems`。每次起播（`playAudio`/`playAudioPlaylist`/
`playAllAudios`/queue 推进）都记录：同曲目 `playCount` 累加并刷新时间。

## Mutation

| Mutation | 说明 |
|---|---|
| `playAudio(path: String!): PlaylistAudio!` | 标记当前曲 + 入手动队列（去重）+ 记录播放。元数据取媒体索引行，未索引则直接探测文件 tag，标题回退文件名去扩展名 |
| `addPlaylistAudios(query: String!): Boolean!` | 搜索 DSL（或 `ids:a,b,c`）解析出的曲目（≤1000）追加进手动队列 |
| `reorderPlaylistAudios(paths: [String!]!): Boolean!` | 手动队列拖拽重排；未知 path 保持原顺序排在末尾 |
| `deletePlaylistAudio(path: String!): Boolean!` | 从手动队列移除一首（不动文件） |
| `clearAudioPlaylist: Boolean!` | 清空 source、手动队列与当前曲 |
| `updateAudioPlayMode(mode: MediaPlayMode!): Boolean!` | 播放模式 |
| `createAudioPlaylist(name: String!): AudioPlaylist!` | 新建用户播放列表 |
| `renameAudioPlaylist(id: String!, name: String!): Boolean!` | 改名（不存在的 id 静默成功，同手机） |
| `deleteAudioPlaylist(id: String!): Boolean!` | 删除列表及其条目；若它是当前 source 则重置 source |
| `addAudioPlaylistItems(id: String!, paths: [String!]!): Boolean!` | 按 path 加曲目，同列表内去重 |
| `removeAudioPlaylistItem(id: String!, path: String!): Boolean!` | 移除一首 |
| `playAudioPlaylist(id: String!, path: String, shuffle: Boolean!): Boolean!` | 以该列表为 source 开始播放（`path` 指定起始曲，缺省第一首；shuffle 时随机推进一次）；空列表只重置 source |
| `playAllAudios(shuffle: Boolean!, path: String): Boolean!` | 以整个曲库为 source（DATE_DESC 顺序，`path` 定位起始曲，shuffle 随机起点） |

## 级联与存储

- 媒体 trash/delete（AUDIO 类型）会把相关 path 从手动队列、播放历史、所有用户
  播放列表中剪除，当前曲被删则清空 current（对齐 plain-app
  `AudioQueueManager.removePaths`）。
- 存储 = SQLite `<data_dir>/plain.db`（plain-rs `library` feature，表名对齐
  plain-app Room 表）：`audio_queue_source`（单行状态，id=1）、`audio_queue_items`、
  `audio_playlists`、`audio_playlist_items`、`audio_play_history`、`library_prefs`；
  播放模式是 `library_prefs` 行 `audio_play_mode`，当前曲在 source 行的
  `current_path` 上（同 plain-app `DAudioQueueSource.currentPath`）。行为代码
  （顺序/supersede/分页/裁剪）在 plain-rs，与 plain-desktop 共用；
  媒体侧缝 `plain-rs/src/media/library_tracks.rs` 实现 `LibraryTracks`（tantivy 索引 + 元数据水化）。

## 已知 parity 备注

- **quirk 复刻**：当"自然下一首"恰好是已手动入队且存在于 source 的曲目时，
  `resolveNext`（playAudioPlaylist shuffle 路径内部）按 path 判 superseded 会把
  手动槽一并否决而返回 null——与 plain-app `AudioQueueManager.resolveNext` 逐行
  一致，测试 `resolve_next_walks_order_and_skips_superseded` 已锁死该行为。
- `Audio.isFavorite` 字段已补（NAS 无单曲收藏，恒 `false`）；web 端 `audios` 页
  sortBy 为可选（plain-app 必填，超集兼容）。
- NAS 不启动播放器：`playAudioPlaylist`/`playAllAudios` 只更新服务端队列状态并
  记录历史，web 客户端轮询 `audioQueueItems`/`app.audioCurrent` 起播。
