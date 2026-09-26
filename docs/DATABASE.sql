-- plain-desktop 本地 SQLite DDL（local_chat.db，WAL）
-- 自动生成，禁止手改。源：plain-rs/src/chat/db/mod.rs
-- 再生：node scripts/gen-db-schema-sql.mjs --write
-- 过期锁：yarn docs:check（vitest docs project）
CREATE TABLE IF NOT EXISTS chats (
id          TEXT PRIMARY KEY,
from_id     TEXT NOT NULL DEFAULT '',
to_id       TEXT NOT NULL DEFAULT '',
channel_id  TEXT NOT NULL DEFAULT '',
content     TEXT NOT NULL DEFAULT '{}',
status      TEXT NOT NULL DEFAULT 'SENT',
status_data TEXT NOT NULL DEFAULT '',
created_at  TEXT NOT NULL DEFAULT '',
updated_at  TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_chats_from_id    ON chats(from_id);
CREATE INDEX IF NOT EXISTS idx_chats_to_id      ON chats(to_id);
CREATE INDEX IF NOT EXISTS idx_chats_channel_id ON chats(channel_id);
CREATE TABLE IF NOT EXISTS chat_channels (
id         TEXT PRIMARY KEY,
name       TEXT NOT NULL DEFAULT '',
owner_id   TEXT NOT NULL DEFAULT 'me',
members    TEXT NOT NULL DEFAULT '[]',
key        TEXT NOT NULL DEFAULT '',
version    INTEGER NOT NULL DEFAULT 1,
status     TEXT NOT NULL DEFAULT 'JOINED',
created_at TEXT NOT NULL DEFAULT '',
updated_at TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS peers (
id          TEXT PRIMARY KEY,
name        TEXT NOT NULL DEFAULT '',
ip          TEXT NOT NULL DEFAULT '',
key         TEXT NOT NULL DEFAULT '',
public_key  TEXT NOT NULL DEFAULT '',
status      TEXT NOT NULL DEFAULT 'UNPAIRED',
port        INTEGER NOT NULL DEFAULT 0,
device_type TEXT NOT NULL DEFAULT '',
token       TEXT NOT NULL DEFAULT '',
created_at  TEXT NOT NULL DEFAULT '',
updated_at  TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS nearby_device_cache (
id          TEXT PRIMARY KEY,
name        TEXT NOT NULL DEFAULT '',
ips         TEXT NOT NULL DEFAULT '',
port        INTEGER NOT NULL DEFAULT 0,
device_type TEXT NOT NULL DEFAULT '',
version     TEXT NOT NULL DEFAULT '',
platform    TEXT NOT NULL DEFAULT '',
last_seen   INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_nearby_device_cache_last_seen ON nearby_device_cache(last_seen DESC);
CREATE TABLE IF NOT EXISTS app_files (
id          TEXT PRIMARY KEY,
size        INTEGER NOT NULL DEFAULT 0,
mime_type   TEXT NOT NULL DEFAULT '',
real_path   TEXT NOT NULL DEFAULT '',
ref_count   INTEGER NOT NULL DEFAULT 1,
weak_hash   TEXT NOT NULL DEFAULT '',
created_at  TEXT NOT NULL DEFAULT '',
updated_at  TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_app_files_weak ON app_files(size, weak_hash);
CREATE TABLE IF NOT EXISTS bookmarks (
id TEXT PRIMARY KEY, url TEXT NOT NULL DEFAULT '', title TEXT NOT NULL DEFAULT '',
favicon_path TEXT NOT NULL DEFAULT '', group_id TEXT NOT NULL DEFAULT '',
pinned INTEGER NOT NULL DEFAULT 0, click_count INTEGER NOT NULL DEFAULT 0,
last_clicked_at TEXT, sort_order INTEGER NOT NULL DEFAULT 0,
created_at TEXT NOT NULL DEFAULT '', updated_at TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_bookmarks_group_id ON bookmarks(group_id);
CREATE TABLE IF NOT EXISTS bookmark_groups (
id TEXT PRIMARY KEY, name TEXT NOT NULL DEFAULT '',
collapsed INTEGER NOT NULL DEFAULT 0, sort_order INTEGER NOT NULL DEFAULT 0,
created_at TEXT NOT NULL DEFAULT '', updated_at TEXT NOT NULL DEFAULT ''
);
