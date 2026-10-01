-- plain-desktop 本地 SQLite DDL（统一 SQLite，WAL）
-- 自动生成，禁止手改。源：plain-rs/src/db/schema/*.rs
-- 再生：node scripts/gen-db-schema-sql.mjs --write
-- 过期锁：yarn docs:check（vitest docs project）
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
CREATE TABLE IF NOT EXISTS audio_queue_source (
id            INTEGER PRIMARY KEY CHECK (id = 1),
source        TEXT    NOT NULL DEFAULT 'NONE',
playlist_id   TEXT    NOT NULL DEFAULT '',
current_path  TEXT    NOT NULL DEFAULT '',
current_index INTEGER NOT NULL DEFAULT -1,
sort_by       TEXT    NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS audio_queue_items (
path          TEXT PRIMARY KEY,
sort_order    INTEGER NOT NULL DEFAULT 0,
title         TEXT    NOT NULL DEFAULT '',
artist        TEXT    NOT NULL DEFAULT '',
duration_secs INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS audio_playlists (
id         TEXT PRIMARY KEY,
name       TEXT    NOT NULL DEFAULT '',
created_at TEXT    NOT NULL DEFAULT '',
updated_at TEXT    NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS audio_playlist_items (
id            TEXT PRIMARY KEY,
playlist_id   TEXT    NOT NULL DEFAULT '',
audio_path    TEXT    NOT NULL DEFAULT '',
title         TEXT    NOT NULL DEFAULT '',
artist        TEXT    NOT NULL DEFAULT '',
duration_secs INTEGER NOT NULL DEFAULT 0,
sort_order    INTEGER NOT NULL DEFAULT 0,
added_at      TEXT    NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_audio_playlist_items_playlist
ON audio_playlist_items(playlist_id, sort_order);
CREATE TABLE IF NOT EXISTS audio_play_history (
path          TEXT PRIMARY KEY,
title         TEXT    NOT NULL DEFAULT '',
artist        TEXT    NOT NULL DEFAULT '',
duration_secs INTEGER NOT NULL DEFAULT 0,
play_count    INTEGER NOT NULL DEFAULT 0,
played_at     TEXT    NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_audio_play_history_played
ON audio_play_history(played_at DESC);
CREATE TABLE IF NOT EXISTS library_prefs (
key   TEXT PRIMARY KEY,
value TEXT NOT NULL DEFAULT ''
);
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
CREATE TABLE IF NOT EXISTS favorite_folders (
root_path     TEXT NOT NULL,
relative_path TEXT NOT NULL,
alias         TEXT,
PRIMARY KEY (root_path, relative_path)
);
CREATE TABLE IF NOT EXISTS feeds (
id TEXT PRIMARY KEY, name TEXT NOT NULL DEFAULT '', url TEXT NOT NULL UNIQUE,
fetch_content INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS feed_entries (
id TEXT PRIMARY KEY, feed_id TEXT NOT NULL, title TEXT NOT NULL DEFAULT '',
url TEXT NOT NULL DEFAULT '', image TEXT NOT NULL DEFAULT '', description TEXT NOT NULL DEFAULT '',
author TEXT NOT NULL DEFAULT '', content TEXT NOT NULL DEFAULT '', raw_id TEXT NOT NULL DEFAULT '',
published_at TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_feed_entries_raw ON feed_entries(feed_id,raw_id);
CREATE INDEX IF NOT EXISTS idx_feed_entries_published ON feed_entries(published_at DESC);
CREATE TABLE IF NOT EXISTS image_editor_projects (
id TEXT PRIMARY KEY,
state_b64 TEXT NOT NULL DEFAULT '',
thumbnail TEXT,
canvas_width INTEGER NOT NULL DEFAULT 0,
canvas_height INTEGER NOT NULL DEFAULT 0,
layer_count INTEGER NOT NULL DEFAULT 0,
created_at TEXT NOT NULL,
updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_image_editor_projects_updated
ON image_editor_projects(updated_at DESC);
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
CREATE TABLE IF NOT EXISTS notes (
id TEXT PRIMARY KEY, title TEXT NOT NULL DEFAULT '', content TEXT NOT NULL DEFAULT '',
deleted_at TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_notes_updated ON notes(updated_at DESC);
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
CREATE TABLE IF NOT EXISTS tags (
id   TEXT PRIMARY KEY,
type INTEGER NOT NULL DEFAULT 0,
name TEXT    NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS tag_relations (
tag_id TEXT NOT NULL DEFAULT '',
key    TEXT NOT NULL DEFAULT '',
PRIMARY KEY (tag_id, key)
);
CREATE INDEX IF NOT EXISTS idx_tag_relations_key ON tag_relations(key);
