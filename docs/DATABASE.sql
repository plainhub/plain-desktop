-- plain-desktop 本地 SQLite DDL（统一 SQLite，WAL）
-- 自动生成，禁止手改。源：plain-rs/src/db/schema/*.rs
-- 再生：node scripts/gen-db-schema-sql.mjs --write
-- 过期锁：yarn docs:check（vitest docs project）
CREATE TABLE IF NOT EXISTS app_files (
id TEXT NOT NULL PRIMARY KEY,
size INTEGER NOT NULL,
mime_type TEXT NOT NULL,
real_path TEXT NOT NULL,
ref_count INTEGER NOT NULL,
weak_hash TEXT NOT NULL,
created_at TEXT NOT NULL,
updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_app_files_weak ON app_files(size, weak_hash);
CREATE TABLE IF NOT EXISTS archived_conversations (
conversation_id TEXT NOT NULL PRIMARY KEY,
conversation_date TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS audio_playback (
id INTEGER NOT NULL PRIMARY KEY CHECK (id=1),
path TEXT NOT NULL,
position_ms INTEGER NOT NULL CHECK (position_ms>=0),
revision INTEGER NOT NULL CHECK (revision>=0),
started_revision INTEGER NOT NULL DEFAULT -1
);
CREATE TABLE IF NOT EXISTS audio_queue_source (
id INTEGER NOT NULL,
source TEXT NOT NULL,
playlist_id TEXT NOT NULL,
current_path TEXT NOT NULL,
current_index INTEGER NOT NULL,
sort_by TEXT NOT NULL,
PRIMARY KEY(id)
);
CREATE TABLE IF NOT EXISTS audio_queue_items (
path TEXT NOT NULL,
sort_order INTEGER NOT NULL,
title TEXT NOT NULL,
artist TEXT NOT NULL,
duration_ms INTEGER NOT NULL,
PRIMARY KEY(path)
);
CREATE INDEX IF NOT EXISTS idx_audio_queue_items_sort_order ON audio_queue_items(sort_order);
CREATE TABLE IF NOT EXISTS audio_playlists (
id TEXT NOT NULL,
name TEXT NOT NULL,
created_at TEXT NOT NULL,
updated_at TEXT NOT NULL,
PRIMARY KEY(id)
);
CREATE TABLE IF NOT EXISTS audio_playlist_items (
id TEXT NOT NULL,
playlist_id TEXT NOT NULL,
audio_path TEXT NOT NULL,
title TEXT NOT NULL,
artist TEXT NOT NULL,
album_id TEXT NOT NULL DEFAULT '',
duration_ms INTEGER NOT NULL,
sort_order INTEGER NOT NULL,
added_at TEXT NOT NULL,
PRIMARY KEY(id)
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_audio_playlist_items_unique ON audio_playlist_items(playlist_id, audio_path);
CREATE INDEX IF NOT EXISTS idx_audio_playlist_items_playlist ON audio_playlist_items(playlist_id, sort_order);
CREATE TABLE IF NOT EXISTS audio_play_history (
path TEXT NOT NULL,
title TEXT NOT NULL,
artist TEXT NOT NULL,
duration_ms INTEGER NOT NULL,
play_count INTEGER NOT NULL DEFAULT 0,
played_at TEXT NOT NULL,
PRIMARY KEY(path)
);
CREATE INDEX IF NOT EXISTS idx_audio_play_history_played ON audio_play_history(played_at);
CREATE TABLE IF NOT EXISTS bookmarks (
id TEXT NOT NULL, url TEXT NOT NULL, title TEXT NOT NULL,
favicon_path TEXT NOT NULL, group_id TEXT NOT NULL,
pinned INTEGER NOT NULL, click_count INTEGER NOT NULL,
last_clicked_at TEXT, sort_order INTEGER NOT NULL,
created_at TEXT NOT NULL, updated_at TEXT NOT NULL, PRIMARY KEY(id)
);
CREATE INDEX IF NOT EXISTS idx_bookmarks_group_id ON bookmarks(group_id);
CREATE TABLE IF NOT EXISTS bookmark_groups (
id TEXT NOT NULL, name TEXT NOT NULL,
collapsed INTEGER NOT NULL, sort_order INTEGER NOT NULL,
created_at TEXT NOT NULL, updated_at TEXT NOT NULL, PRIMARY KEY(id)
);
CREATE TABLE IF NOT EXISTS chats (
id TEXT NOT NULL,
from_id TEXT NOT NULL,
to_id TEXT NOT NULL,
channel_id TEXT NOT NULL,
status TEXT NOT NULL DEFAULT 'PENDING',
status_data TEXT NOT NULL DEFAULT '',
content TEXT NOT NULL,
created_at TEXT NOT NULL,
updated_at TEXT NOT NULL,
PRIMARY KEY(id)
);
CREATE INDEX IF NOT EXISTS idx_chats_from_id ON chats(from_id);
CREATE INDEX IF NOT EXISTS idx_chats_to_id ON chats(to_id);
CREATE INDEX IF NOT EXISTS idx_chats_channel_id ON chats(channel_id);
CREATE TABLE IF NOT EXISTS chat_channels (
id TEXT NOT NULL,
name TEXT NOT NULL,
key TEXT NOT NULL,
owner_id TEXT NOT NULL DEFAULT '',
members TEXT NOT NULL,
version INTEGER NOT NULL DEFAULT 0,
status TEXT NOT NULL DEFAULT 'JOINED',
created_at TEXT NOT NULL,
updated_at TEXT NOT NULL,
PRIMARY KEY(id)
);
CREATE TABLE IF NOT EXISTS clipboards (
id TEXT NOT NULL PRIMARY KEY,
text TEXT NOT NULL,
hash TEXT NOT NULL,
source TEXT NOT NULL,
label TEXT NOT NULL,
sensitive INTEGER NOT NULL,
created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_clipboards_hash ON clipboards(hash);
CREATE TABLE IF NOT EXISTS favorite_folders (
root_path     TEXT NOT NULL,
relative_path TEXT NOT NULL,
alias         TEXT,
PRIMARY KEY (root_path, relative_path)
);
CREATE TABLE IF NOT EXISTS feeds (
id TEXT NOT NULL PRIMARY KEY, name TEXT NOT NULL, url TEXT NOT NULL,
logo TEXT NOT NULL DEFAULT '', fetch_content INTEGER NOT NULL, last_sync_at TEXT,
last_error TEXT NOT NULL DEFAULT '', created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS feed_entries (
id TEXT NOT NULL PRIMARY KEY, title TEXT NOT NULL, url TEXT NOT NULL, image TEXT NOT NULL,
description TEXT NOT NULL, author TEXT NOT NULL, content TEXT NOT NULL, feed_id TEXT NOT NULL,
raw_id TEXT NOT NULL, published_at TEXT NOT NULL, read INTEGER NOT NULL,
created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_feed_entries_feed_id ON feed_entries(feed_id);
CREATE INDEX IF NOT EXISTS idx_feed_entries_raw_id ON feed_entries(raw_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_feeds_url ON feeds(url);
CREATE TABLE IF NOT EXISTS image_editor_projects (
id TEXT NOT NULL PRIMARY KEY,
state_b64 TEXT NOT NULL,
thumbnail TEXT,
canvas_width INTEGER NOT NULL,
canvas_height INTEGER NOT NULL,
layer_count INTEGER NOT NULL,
created_at TEXT NOT NULL,
updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_image_editor_projects_updated
ON image_editor_projects(updated_at DESC);
CREATE TABLE IF NOT EXISTS image_embeddings (
id TEXT NOT NULL PRIMARY KEY,
path TEXT NOT NULL,
embedding BLOB NOT NULL,
created_at TEXT NOT NULL,
updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS media_item (
media_type TEXT NOT NULL,
media_id TEXT NOT NULL PRIMARY KEY,
duration_ms INTEGER NOT NULL,
updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS nearby_device_cache (
id TEXT NOT NULL PRIMARY KEY,
name TEXT NOT NULL,
ips TEXT NOT NULL,
port INTEGER NOT NULL,
device_type TEXT NOT NULL,
version TEXT NOT NULL,
platform TEXT NOT NULL,
last_seen TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_nearby_device_cache_last_seen ON nearby_device_cache(last_seen DESC);
CREATE TABLE IF NOT EXISTS notes (
id TEXT NOT NULL PRIMARY KEY, title TEXT NOT NULL, content TEXT NOT NULL,
deleted_at TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_notes_updated ON notes(updated_at DESC);
CREATE TABLE IF NOT EXISTS peers (
id TEXT NOT NULL PRIMARY KEY,
name TEXT NOT NULL,
ip TEXT NOT NULL,
key TEXT NOT NULL,
public_key TEXT NOT NULL,
status TEXT NOT NULL,
port INTEGER NOT NULL,
device_type TEXT NOT NULL,
token TEXT NOT NULL DEFAULT '',
created_at TEXT NOT NULL,
updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS pomodoro_items (
id TEXT NOT NULL PRIMARY KEY,
date TEXT NOT NULL,
completed_count INTEGER NOT NULL,
total_work_seconds INTEGER NOT NULL,
total_break_seconds INTEGER NOT NULL,
created_at TEXT NOT NULL,
updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS pomodoro_runtime (id INTEGER PRIMARY KEY CHECK(id=1), data TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS sessions (
client_id TEXT NOT NULL PRIMARY KEY,
name TEXT NOT NULL DEFAULT '',
type TEXT NOT NULL DEFAULT 'WEB',
client_ip TEXT NOT NULL,
os_name TEXT NOT NULL,
os_version TEXT NOT NULL,
browser_name TEXT NOT NULL,
browser_version TEXT NOT NULL,
token TEXT NOT NULL,
last_active_at TEXT,
created_at TEXT NOT NULL,
updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS shares (
id TEXT NOT NULL PRIMARY KEY,
name TEXT NOT NULL,
password TEXT NOT NULL,
url_token TEXT NOT NULL,
expires_at TEXT,
read_only INTEGER NOT NULL,
data TEXT NOT NULL,
created_at TEXT NOT NULL,
updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS tags (
id TEXT NOT NULL PRIMARY KEY,
name TEXT NOT NULL,
type INTEGER NOT NULL,
count INTEGER NOT NULL,
created_at TEXT NOT NULL,
updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS tag_relations (
tag_id TEXT NOT NULL,
key TEXT NOT NULL,
type INTEGER NOT NULL,
created_at TEXT NOT NULL,
size INTEGER NOT NULL,
title TEXT NOT NULL,
PRIMARY KEY (tag_id, key, type)
);
CREATE INDEX IF NOT EXISTS idx_tag_relations_key ON tag_relations(key);
CREATE TABLE IF NOT EXISTS trashed_sms (
message_id TEXT NOT NULL PRIMARY KEY,
is_mms INTEGER NOT NULL,
trashed_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS video_play_progress (
media_id TEXT NOT NULL PRIMARY KEY,
position_ms INTEGER NOT NULL,
updated_at TEXT NOT NULL
);
