# plain-rs 与 plain-app 数据库字段差异

对比来源：plain-rs `docs/DATABASE.sql`（由 `plain-rs/src/db/schema/` 生成）与 plain-app `../plain-app/shared/apitest/DATABASE.sql`（由 Room schema 生成）。字段类型、空值约束、默认值和主键按当前 DDL 比较。

## 对比结果

| 项目 | plain-rs | plain-app |
|---|---:|---:|
| 表数 | 29 | 29 |
| 同名表 | 25 | 25 |
| 两边未对齐名称的表 | `favorite_folders`, `settings` | `book_chapters`, `books`（plain-rs 按要求不实现 book 功能） |

两边有 27 组功能表对应关系（25 张同名表，加上以下两组用户指定的表名映射）。表名映射为：plain-app `files` 对应 plain-rs `app_files`；plain-app `trashed_messages` 对应 plain-rs `trashed_sms`。这两组字段完全一致。

## 字段差异

| 表 | 差异 |
|---|---|
| `peers` | plain-rs 保留额外字段 `token TEXT NOT NULL DEFAULT ''`；plain-app 没有该字段。 |

除上述 `peers.token` 外，所有同名表及两组映射表的字段集合、SQLite 类型、空值约束和默认值均与 plain-app 一致。plain-app 的 `books` 与 `book_chapters` 不在 plain-rs 中创建。音频字段现统一为 `duration_ms`；`nearby_device_cache.last_seen` 为 `TEXT`，按 ISO 时间字符串保存。

## plain-rs 额外表

| 表 | 字段 | 用途 |
|---|---|---|
| `favorite_folders` | `root_path`, `relative_path`, `alias` | plain-rs 资料库收藏目录。 |
| `settings` | `key`, `value` | 用户设置。`prefs.json` 继续保存由 app 生成和管理的配置。 |

## plain-rs 当前字段清单

| 表 | 字段 |
|---|---|
| `app_files`（plain-app `files`） | `id`, `size`, `mime_type`, `real_path`, `ref_count`, `weak_hash`, `created_at`, `updated_at` |
| `archived_conversations` | `conversation_id`, `conversation_date` |
| `audio_play_history` | `path`, `title`, `artist`, `duration_ms`, `play_count`, `played_at` |
| `audio_playlist_items` | `id`, `playlist_id`, `audio_path`, `title`, `artist`, `album_id`, `duration_ms`, `sort_order`, `added_at` |
| `audio_playlists` | `id`, `name`, `created_at`, `updated_at` |
| `audio_queue_items` | `path`, `sort_order`, `title`, `artist`, `duration_ms` |
| `audio_queue_source` | `id`, `source`, `playlist_id`, `current_path`, `current_index`, `sort_by` |
| `bookmark_groups` | `id`, `name`, `collapsed`, `sort_order`, `created_at`, `updated_at` |
| `bookmarks` | `id`, `url`, `title`, `favicon_path`, `group_id`, `pinned`, `click_count`, `last_clicked_at`, `sort_order`, `created_at`, `updated_at` |
| `chat_channels` | `id`, `name`, `key`, `owner_id`, `members`, `version`, `status`, `created_at`, `updated_at` |
| `chats` | `id`, `from_id`, `to_id`, `channel_id`, `status`, `status_data`, `content`, `created_at`, `updated_at` |
| `clipboards` | `id`, `text`, `hash`, `source`, `label`, `sensitive`, `created_at` |
| `favorite_folders` | `root_path`, `relative_path`, `alias` |
| `feed_entries` | `id`, `title`, `url`, `image`, `description`, `author`, `content`, `feed_id`, `raw_id`, `published_at`, `read`, `created_at`, `updated_at` |
| `feeds` | `id`, `name`, `url`, `logo`, `fetch_content`, `last_sync_at`, `last_error`, `created_at`, `updated_at` |
| `image_editor_projects` | `id`, `state_b64`, `thumbnail`, `canvas_width`, `canvas_height`, `layer_count`, `created_at`, `updated_at` |
| `image_embeddings` | `id`, `path`, `embedding`, `created_at`, `updated_at` |
| `media_item` | `media_type`, `media_id`, `duration_ms`, `updated_at` |
| `nearby_device_cache` | `id`, `name`, `ips`, `port`, `device_type`, `version`, `platform`, `last_seen` |
| `notes` | `id`, `title`, `content`, `deleted_at`, `created_at`, `updated_at` |
| `peers` | `id`, `name`, `ip`, `key`, `public_key`, `status`, `port`, `device_type`, `token`, `created_at`, `updated_at` |
| `pomodoro_items` | `id`, `date`, `completed_count`, `total_work_seconds`, `total_break_seconds`, `created_at`, `updated_at` |
| `sessions` | `client_id`, `name`, `type`, `client_ip`, `os_name`, `os_version`, `browser_name`, `browser_version`, `token`, `last_active_at`, `created_at`, `updated_at` |
| `settings` | `key`, `value` |
| `shares` | `id`, `name`, `password`, `url_token`, `expires_at`, `read_only`, `data`, `created_at`, `updated_at` |
| `tag_relations` | `tag_id`, `key`, `type`, `created_at`, `size`, `title` |
| `tags` | `id`, `name`, `type`, `count`, `created_at`, `updated_at` |
| `trashed_sms`（plain-app `trashed_messages`） | `message_id`, `is_mms`, `trashed_at` |
| `video_play_progress` | `media_id`, `position_ms`, `updated_at` |

## 说明

- `library_prefs` 已改名为 `settings`，只用于用户设置；app 管理配置仍在 `prefs.json`。
- plain-rs 没有另建 `files` 表，继续使用 `app_files` 保存与 plain-app `files` 对应的数据。
- schema 变更只更新当前建表定义和正常读写代码；未加入旧数据库迁移或兼容逻辑。
