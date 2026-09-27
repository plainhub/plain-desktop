//! `Mutation` root. The set of resolvers here mirrors the Go schema in
//! `internal/graph/schema.graphql` (see `cmd/services/api/schema.resolvers.go`).
//! Every type signature here matches the Go `schema.graphql` SDL.

use super::types::*;
use async_graphql::{Context, FieldResult, ID, Object};
use std::process::Command;

pub struct NasMutationRoot;

/// The HTTPS port this server advertises in pairing traffic (LAN peers
/// talk `https://ip:port`); falls back to the plain-app default 8443
/// when the config leaves it blank.
fn pairing_local_port(ctx: &Context<'_>) -> FieldResult<u16> {
    let config = ctx.data::<std::sync::Arc<crate::media::config::Config>>()?;
    let raw = config.get_string("server.https_port");
    Ok(raw.parse::<u16>().unwrap_or(8443))
}

fn chat_state(ctx: &Context<'_>) -> FieldResult<std::sync::Arc<crate::api::chat::ChatState>> {
    ctx.data::<std::sync::Arc<crate::api::chat::ChatState>>()
        .cloned()
        .map_err(|_| async_graphql::Error::new("chat state unavailable"))
}

/// Background merge jobs, keyed by fileId — mirrors plain-app `MergeJobs`.
/// DONE keeps `value`="name" and `merged_size`; FAILED keeps the message.
enum MergeJobState {
    Merging,
    Done { value: String, merged_size: i64 },
    Failed(String),
}

fn merge_jobs() -> &'static std::sync::Mutex<std::collections::HashMap<String, MergeJobState>> {
    static JOBS: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, MergeJobState>>,
    > = std::sync::OnceLock::new();
    JOBS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

pub(crate) fn merge_job_status_pub(file_id: &str) -> MergeTask {
    merge_job_status(file_id)
}

fn merge_job_status(file_id: &str) -> MergeTask {
    let jobs = merge_jobs().lock().unwrap();
    match jobs.get(file_id) {
        None => MergeTask {
            status: MergeTaskStatus::NONE,
            value: None,
            merged_size: None,
            error: None,
        },
        Some(MergeJobState::Merging) => MergeTask {
            status: MergeTaskStatus::MERGING,
            value: None,
            merged_size: None,
            error: None,
        },
        Some(MergeJobState::Done { value, merged_size }) => MergeTask {
            status: MergeTaskStatus::DONE,
            value: Some(value.clone()),
            merged_size: Some(Long(*merged_size)),
            error: None,
        },
        Some(MergeJobState::Failed(e)) => MergeTask {
            status: MergeTaskStatus::FAILED,
            value: None,
            merged_size: None,
            error: Some(e.clone()),
        },
    }
}

#[Object]
impl NasMutationRoot {
    /// Set the OS hostname. Mirrors Go `setDeviceNameModel`:
    /// `hostnamectl set-hostname` + `systemctl restart avahi-daemon`.
    /// (Named `setHostname` to disambiguate it from the plain-app
    /// `updateDeviceName`, which only changes the display-name preference.)
    async fn set_hostname(&self, ctx: &Context<'_>, name: String) -> FieldResult<bool> {
        let sanitized = sanitize_hostname(&name);
        if sanitized.is_empty() {
            return Err(async_graphql::Error::new("device_name_invalid"));
        }
        let cid: String = ctx.data::<String>()?.clone();
        let db = ctx.data::<std::sync::Arc<crate::media::kv::Db>>()?;
        let hostnamectl = Command::new("hostnamectl")
            .arg("set-hostname")
            .arg(&sanitized)
            .status();
        match hostnamectl {
            Ok(s) if s.success() => {}
            _ => {
                return Err(async_graphql::Error::new("device_name_set_hostname_failed"));
            }
        }
        // Best-effort avahi restart; missing avahi is not fatal.
        let _ = Command::new("systemctl")
            .args(["try-restart", "avahi-daemon"])
            .status();
        let _ = crate::media::kv::EventLog::new(db).add("set_hostname", &sanitized, &cid);
        Ok(true)
    }

    /// Update the device display name. Mirrors plain-app `updateDeviceName`:
    /// stores a display-name preference served back via `App.deviceName` —
    /// the OS hostname is left alone (that is `setHostname`, which needs
    /// root). An empty name clears the override, falling back to the hostname.
    async fn update_device_name(&self, ctx: &Context<'_>, name: String) -> FieldResult<bool> {
        let cid: String = ctx.data::<String>()?.clone();
        let prefs = ctx.data::<std::sync::Arc<crate::prefs::Prefs>>()?;
        let db = ctx.data::<std::sync::Arc<crate::media::kv::Db>>()?;
        let name = name.trim();
        prefs.set("device_name", name)?;
        let _ = crate::media::kv::EventLog::new(db).add("update_device_name", name, &cid);
        // A rename changes the advertised mDNS instance name — republish
        // so peers drop the old instance and see the new one immediately
        // (plain-app `updateAdvertisedService` parity). The shared chat
        // identity picks the new name up on its next publish.
        let chat = chat_state(ctx)?;
        let display = if name.is_empty() {
            crate::hostname::get()
        } else {
            name.to_string()
        };
        chat.service.identity.set_device_name(&display);
        if let Some(d) = chat.discovery.as_ref() {
            d.update_advertised_service();
        }
        Ok(true)
    }

    // ----- Developer pages (plain-app contract) -----

    /// Truncate the current log file (plain-app `clearAppLogs`).
    async fn clear_app_logs(&self, ctx: &Context<'_>) -> FieldResult<bool> {
        let data_dir = ctx.data::<std::path::PathBuf>()?;
        crate::nas::log::clear_file(&crate::nas::log::default_log_file(data_dir));
        Ok(true)
    }

    /// Delete one preference entry (plain-app `deleteDataStoreEntry` /
    /// plain-desktop `deleteDataStoreEntry` — removes the key when present).
    async fn delete_data_store_entry(&self, ctx: &Context<'_>, key: String) -> FieldResult<bool> {
        let prefs = ctx.data::<std::sync::Arc<crate::prefs::Prefs>>()?;
        prefs.remove(&key)?;
        Ok(true)
    }

    /// Delete rows of one table by their primary key values (plain-app
    /// `deleteDbTableRows`; on a composite-key table every row sharing
    /// the first key column's value goes).
    async fn delete_db_table_rows(
        &self,
        ctx: &Context<'_>,
        table: String,
        ids: Vec<String>,
    ) -> FieldResult<bool> {
        let chat = ctx.data::<std::sync::Arc<crate::api::chat::ChatState>>()?;
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        crate::nas::devtools_sqlite::delete_table_rows(&chat.service.db, library, &table, &ids)
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(true)
    }

    /// Logout the current session. Mirrors Go `logout` which calls
    /// `db.RevokeSession(clientID)` and writes a `logout` audit event.
    async fn logout(&self, ctx: &Context<'_>) -> FieldResult<bool> {
        let cid: String = ctx.data::<String>()?.clone();
        let db = ctx.data::<std::sync::Arc<crate::media::kv::Db>>()?;
        let name = crate::media::kv::SessionStore::new(db)
            .get(&cid)
            .map(|s| s.client_name)
            .unwrap_or_default();
        let _ = crate::media::kv::EventLog::new(db).add("logout", &name, &cid);
        let _ = crate::media::kv::SessionStore::new(db).delete(&cid);
        Ok(true)
    }

    /// Revoke a session by client id. Mirrors Go `revokeSession`.
    async fn revoke_session(&self, ctx: &Context<'_>, client_id: String) -> FieldResult<bool> {
        let caller_cid: String = ctx.data::<String>()?.clone();
        let db = ctx.data::<std::sync::Arc<crate::media::kv::Db>>()?;
        if client_id.trim().is_empty() {
            return Ok(false);
        }
        let name = crate::media::kv::SessionStore::new(db)
            .get(&client_id)
            .map(|s| s.client_name)
            .unwrap_or_default();
        let _ = crate::media::kv::EventLog::new(db).add("revoke", &name, &client_id);
        let _ = crate::media::kv::SessionStore::new(db).delete(&client_id);
        let _ = caller_cid;
        Ok(true)
    }

    // ----- Tags -----

    // ----- Favorite folders (phone contract: fullPath addressing, the
    // mutations return the updated list) -----

    /// Register a favorite folder and return the whole list (plain-app contract shape).
    async fn add_favorite_folder(
        &self,
        ctx: &Context<'_>,
        #[graphql(name = "rootPath")] root_path: String,
        #[graphql(name = "fullPath")] full_path: String,
    ) -> FieldResult<Vec<FavoriteFolder>> {
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        let (root, rel) = crate::library::favorite_folders::split_full_path(&root_path, &full_path);
        crate::library::favorite_folders::add(library, &root, &rel);
        Ok(favorite_list_gql(library))
    }

    /// Remove the favorite folder identified by `fullPath` and return the whole list.
    async fn remove_favorite_folder(
        &self,
        ctx: &Context<'_>,
        #[graphql(name = "fullPath")] full_path: String,
    ) -> FieldResult<Vec<FavoriteFolder>> {
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        if let Some(f) = crate::library::favorite_folders::find_by_full_path(library, &full_path) {
            crate::library::favorite_folders::remove(library, &f.root_path, &f.relative_path);
        }
        Ok(favorite_list_gql(library))
    }

    /// Set the favorite folder's display alias and return the whole list.
    async fn set_favorite_folder_alias(
        &self,
        ctx: &Context<'_>,
        #[graphql(name = "fullPath")] full_path: String,
        alias: String,
    ) -> FieldResult<Vec<FavoriteFolder>> {
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        if let Some(f) = crate::library::favorite_folders::find_by_full_path(library, &full_path) {
            crate::library::favorite_folders::set_alias(
                library,
                &f.root_path,
                &f.relative_path,
                &alias,
            );
        }
        Ok(favorite_list_gql(library))
    }

    // ----- Audio playback queue / user playlists (plain-app AudioGraphQL) -----

    /// Add up to 1000 tracks matching `query` to the manual queue.
    async fn add_audios_to_queue(&self, ctx: &Context<'_>, query: String) -> FieldResult<bool> {
        let db = ctx.data::<std::sync::Arc<crate::media::kv::Db>>()?.clone();
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        let items = {
            let db = db.clone();
            super::run_blocking(move || resolve_queue_tracks(&db, &query, 1000)).await?
        };
        crate::library::audio_queue::enqueue(library, &items, false);
        Ok(true)
    }

    /// Drag & drop reorder of the manual queue; unknown paths keep their
    /// order at the end.
    async fn reorder_audio_queue(
        &self,
        ctx: &Context<'_>,
        paths: Vec<String>,
    ) -> FieldResult<bool> {
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        crate::library::audio_queue::reorder_queued(library, &paths);
        Ok(true)
    }

    /// Play the given track: mark it current, enqueue it in the manual
    /// queue when missing, and record the play (on the phone the native
    /// player records when playback actually starts; the NAS server is the
    /// playback state machine, so it records here).
    async fn play_audio(&self, ctx: &Context<'_>, path: String) -> FieldResult<AudioItem> {
        let db = ctx.data::<std::sync::Arc<crate::media::kv::Db>>()?.clone();
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        let pa = {
            super::run_blocking(move || {
                crate::media::library_tracks::playlist_audio_from_path(&db, &path)
            })
            .await?
        };
        crate::library::audio_queue::enqueue(library, std::slice::from_ref(&pa), false);
        crate::library::audio_queue::on_playing(
            library,
            &pa.path,
            &pa.title,
            &pa.artist,
            pa.duration_secs,
        );
        Ok(AudioItem {
            title: pa.title,
            artist: pa.artist,
            path: pa.path,
            duration_ms: Long(pa.duration_secs * 1000),
        })
    }

    /// Persist the playback mode preference (REPEAT/REPEAT_ONE/SHUFFLE).
    async fn update_audio_play_mode(
        &self,
        ctx: &Context<'_>,
        mode: MediaPlayMode,
    ) -> FieldResult<bool> {
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        let mode_str = match mode {
            MediaPlayMode::REPEAT => "REPEAT",
            MediaPlayMode::REPEAT_ONE => "REPEAT_ONE",
            MediaPlayMode::SHUFFLE => "SHUFFLE",
        };
        crate::library::audio_queue::save_audio_mode(library, mode_str);
        Ok(true)
    }

    /// Reset the source, the manual queue and the current track.
    async fn clear_audio_queue(&self, ctx: &Context<'_>) -> FieldResult<bool> {
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        crate::library::audio_queue::save_audio_current(library, "");
        crate::library::audio_queue::clear_queue(library);
        Ok(true)
    }

    /// Remove a track from the manual queue.
    async fn remove_audio_from_queue(&self, ctx: &Context<'_>, path: String) -> FieldResult<bool> {
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        crate::library::audio_queue::remove_queued(library, &path);
        Ok(true)
    }

    // ----- User playlists -----

    /// Create an empty user playlist and return it.
    async fn create_audio_playlist(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> FieldResult<AudioPlaylist> {
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        let pl = crate::library::audio_queue::create_playlist(library, &name);
        Ok(AudioPlaylist {
            id: pl.id.into(),
            name: pl.name,
            item_count: 0,
            created_at: instant_of(&pl.created_at),
            updated_at: instant_of(&pl.updated_at),
        })
    }

    /// Update a playlist's name; returns the updated playlist. A missing id is
    /// an error, not a silent no-op (phone parity).
    async fn update_audio_playlist(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        name: String,
    ) -> FieldResult<AudioPlaylist> {
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        crate::library::audio_queue::rename_playlist(library, &id, &name);
        let pl = crate::library::audio_queue::playlist_by_id(library, &id).ok_or_else(|| {
            async_graphql::Error::new(format!("Playlist {} not found after update", id.0))
        })?;
        let count = crate::library::audio_queue::playlist_item_count(library, &id);
        Ok(AudioPlaylist {
            id: pl.id.into(),
            name: pl.name,
            item_count: count.min(i32::MAX as usize) as i32,
            created_at: instant_of(&pl.created_at),
            updated_at: instant_of(&pl.updated_at),
        })
    }

    /// Delete a playlist; its items go with it. Single idempotent delete → `Boolean!` (§6).
    async fn delete_audio_playlist(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> FieldResult<bool> {
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        crate::library::audio_queue::delete_playlist(library, &id);
        Ok(true)
    }

    /// Add tracks (by path) to a playlist; duplicates are ignored.
    async fn add_audio_playlist_items(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        paths: Vec<String>,
    ) -> FieldResult<bool> {
        let db = ctx.data::<std::sync::Arc<crate::media::kv::Db>>()?.clone();
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        let items = {
            super::run_blocking(move || {
                paths
                    .iter()
                    .map(|p| crate::media::library_tracks::playlist_audio_from_path(&db, p))
                    .collect::<Vec<_>>()
            })
            .await?
        };
        crate::library::audio_queue::add_playlist_items(library, &id, &items);
        Ok(true)
    }

    /// Remove one track (by path) from a playlist.
    async fn remove_audio_playlist_item(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        path: String,
    ) -> FieldResult<bool> {
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        crate::library::audio_queue::remove_playlist_item(library, &id, &path);
        Ok(true)
    }

    /// Play a user playlist: make it the playback source and resolve the
    /// track to start with (random one when shuffling).
    async fn play_audio_playlist(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        path: Option<String>,
        shuffle: bool,
    ) -> FieldResult<Option<AudioItem>> {
        let db = ctx.data::<std::sync::Arc<crate::media::kv::Db>>()?.clone();
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        let library = (**library).clone();
        let id2 = id.to_string();
        let index = crate::media::image_index::global();
        let started = super::run_blocking(move || {
            let start =
                crate::library::audio_queue::set_playlist_source(&library, &id2, path.as_deref());
            let track = if shuffle {
                match start {
                    Some(_) => {
                        let mut tracks = crate::nas::library::NasLibraryTracks::new(db, index);
                        crate::library::audio_queue::resolve_next(&library, &mut tracks, true, true)
                            .map_err(|e| anyhow::anyhow!(e.to_string()))?
                    }
                    None => None,
                }
            } else {
                start
            };
            Ok::<_, anyhow::Error>(track)
        })
        .await??;
        Ok(started.map(super::query::playlist_audio_to_gql))
    }

    /// Queue the whole audio library and start playback (first track, or
    /// shuffled order when `shuffle` is set). Returns the item that starts
    /// playing, null when the library is empty.
    async fn play_all_audios(
        &self,
        ctx: &Context<'_>,
        shuffle: bool,
    ) -> FieldResult<Option<AudioItem>> {
        let db = ctx.data::<std::sync::Arc<crate::media::kv::Db>>()?.clone();
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        let library = (**library).clone();
        let index = crate::media::image_index::global();
        let started = super::run_blocking(move || {
            let mut tracks = crate::nas::library::NasLibraryTracks::new(db, index);
            crate::library::audio_queue::set_library_source(&library, &mut tracks, None, shuffle)
                .map_err(|e| anyhow::anyhow!(e.to_string()))
        })
        .await??;
        Ok(started.map(super::query::playlist_audio_to_gql))
    }

    // ----- Bookmarks (plain-app surface, backed by db::bookmarks) -----

    /// Create one bookmark per URL in the given group (blank URLs are skipped) and return the created entities.
    async fn add_bookmarks(
        &self,
        ctx: &Context<'_>,
        urls: Vec<String>,
        #[graphql(name = "groupId")] group_id: async_graphql::ID,
    ) -> FieldResult<Vec<Bookmark>> {
        let chat = ctx.data::<std::sync::Arc<crate::api::chat::ChatState>>()?;
        let db = &chat.service.db;
        // Trim, skip empties; title starts as the URL (plain-app
        // BookmarkHelper semantics).
        let created: Vec<_> = urls
            .iter()
            .map(|u| u.trim())
            .filter(|u| !u.is_empty())
            .map(|u| crate::chat::db::bookmark::DBookmark::new(u, &group_id))
            .collect();
        for b in &created {
            crate::chat::db::bookmark::insert_bookmark(db, b);
        }
        Ok(created.into_iter().map(bookmark_to_gql).collect())
    }

    /// Update a bookmark's url/title/group/pin/order and return the updated entity.
    async fn update_bookmark(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        input: BookmarkInput,
    ) -> FieldResult<Bookmark> {
        let chat = ctx.data::<std::sync::Arc<crate::api::chat::ChatState>>()?;
        let db = &chat.service.db;
        let updated = crate::chat::db::bookmark::get_bookmark_by_id(db, &id)
            .map(|mut b| {
                b.url = input.url.clone();
                b.title = input.title.clone();
                b.group_id = input.group_id.to_string();
                b.pinned = input.pinned;
                b.sort_order = input.sort_order;
                b.updated_at = crate::chat::db::now_iso();
                crate::chat::db::bookmark::update_bookmark(db, &b);
                b
            })
            .map(bookmark_to_gql)
            .ok_or_else(|| async_graphql::Error::new(format!("bookmark not found: {}", &*id)))?;
        Ok(updated)
    }

    /// Delete the given bookmarks; `affectedCount` = how many were removed (§6).
    async fn delete_bookmarks(
        &self,
        ctx: &Context<'_>,
        ids: Vec<async_graphql::ID>,
    ) -> FieldResult<ActionResult> {
        let chat = ctx.data::<std::sync::Arc<crate::api::chat::ChatState>>()?;
        let ids: Vec<String> = ids.iter().map(|i| i.to_string()).collect();
        // affectedCount = how many of the requested ids actually existed.
        let affected = crate::chat::db::bookmark::delete_bookmarks(&chat.service.db, &ids);
        Ok(ActionResult {
            affected_count: affected,
        })
    }

    /// Record a click: bumps `clickCount` and sets `lastClickedAt`.
    async fn record_bookmark_click(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> FieldResult<bool> {
        let chat = ctx.data::<std::sync::Arc<crate::api::chat::ChatState>>()?;
        let db = &chat.service.db;
        // Unknown ids are a silent no-op (same as phone).
        if let Some(mut b) = crate::chat::db::bookmark::get_bookmark_by_id(db, &id) {
            b.click_count += 1;
            let now = crate::chat::db::now_iso();
            b.last_clicked_at = Some(now.clone());
            b.updated_at = now;
            crate::chat::db::bookmark::update_bookmark(db, &b);
        }
        Ok(true)
    }

    /// Create a bookmark group and return it.
    async fn create_bookmark_group(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> FieldResult<BookmarkGroup> {
        let chat = ctx.data::<std::sync::Arc<crate::api::chat::ChatState>>()?;
        let g = crate::chat::db::bookmark::DBookmarkGroup::new(&name);
        crate::chat::db::bookmark::insert_bookmark_group(&chat.service.db, &g);
        // A fresh group has no bookmarks yet — no scan needed.
        Ok(bookmark_group_to_gql(g, 0))
    }

    /// Update a bookmark group's name/collapsed/sortOrder and return the updated entity.
    async fn update_bookmark_group(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        name: String,
        collapsed: bool,
        sort_order: i32,
    ) -> FieldResult<BookmarkGroup> {
        let chat = ctx.data::<std::sync::Arc<crate::api::chat::ChatState>>()?;
        let db = &chat.service.db;
        let id_str = id.to_string();
        let item_count = crate::chat::db::bookmark::get_bookmarks_by_group_id(db, &id_str).len();
        let updated = crate::chat::db::bookmark::get_bookmark_group_by_id(db, &id_str)
            .map(|mut g| {
                g.name = name;
                g.collapsed = collapsed;
                g.sort_order = sort_order;
                g.updated_at = crate::chat::db::now_iso();
                crate::chat::db::bookmark::update_bookmark_group(db, &g);
                g
            })
            .map(|g| bookmark_group_to_gql(g, item_count))
            .ok_or_else(|| async_graphql::Error::new(format!("group not found: {}", &*id)))?;
        Ok(updated)
    }

    /// Delete a bookmark group; its bookmarks are kept and become ungrouped (`groupId` cleared).
    async fn delete_bookmark_group(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> FieldResult<bool> {
        let chat = ctx.data::<std::sync::Arc<crate::api::chat::ChatState>>()?;
        // Member bookmarks move to ungrouped (phone deleteGroup semantics
        // — implemented in the shared core).
        crate::chat::db::bookmark::delete_bookmark_group(&chat.service.db, &id);
        Ok(true)
    }

    // ----- Chat (plain-app contract, backed by crate::chat) -----

    async fn create_chat_channel(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> FieldResult<ChatChannel> {
        let chat = chat_state(ctx)?;
        Ok(crate::api::schema::nas::types::chat_channel_from_dchannel(
            chat.service.create_channel(&name),
        ))
    }

    async fn update_chat_channel(
        &self,
        ctx: &Context<'_>,
        id: ID,
        name: String,
    ) -> FieldResult<ChatChannel> {
        let chat = chat_state(ctx)?;
        let ch = chat
            .service
            .update_channel_name(&id.to_string(), &name)
            .await
            .map_err(async_graphql::Error::new)?;
        Ok(crate::api::schema::nas::types::chat_channel_from_dchannel(
            ch,
        ))
    }

    async fn delete_chat_channel(&self, ctx: &Context<'_>, id: ID) -> FieldResult<bool> {
        let chat = chat_state(ctx)?;
        Ok(chat.service.delete_channel(&id.to_string()).await)
    }

    async fn leave_chat_channel(&self, ctx: &Context<'_>, id: ID) -> FieldResult<bool> {
        let chat = chat_state(ctx)?;
        Ok(chat.service.leave_channel(&id.to_string()).await)
    }

    async fn add_chat_channel_member(
        &self,
        ctx: &Context<'_>,
        id: ID,
        #[graphql(name = "peerId")] peer_id: ID,
    ) -> FieldResult<ChatChannel> {
        let chat = chat_state(ctx)?;
        let ch = chat
            .service
            .add_channel_member(&id.to_string(), &peer_id.to_string())
            .await
            .map_err(async_graphql::Error::new)?;
        Ok(crate::api::schema::nas::types::chat_channel_from_dchannel(
            ch,
        ))
    }

    async fn remove_chat_channel_member(
        &self,
        ctx: &Context<'_>,
        id: ID,
        #[graphql(name = "peerId")] peer_id: ID,
    ) -> FieldResult<ChatChannel> {
        let chat = chat_state(ctx)?;
        let ch = chat
            .service
            .remove_channel_member(&id.to_string(), &peer_id.to_string())
            .await
            .map_err(async_graphql::Error::new)?;
        Ok(crate::api::schema::nas::types::chat_channel_from_dchannel(
            ch,
        ))
    }

    async fn accept_chat_channel_invite(&self, ctx: &Context<'_>, id: ID) -> FieldResult<bool> {
        let chat = chat_state(ctx)?;
        chat.service
            .accept_channel_invite(&id.to_string())
            .await
            .map_err(async_graphql::Error::new)
    }

    async fn decline_chat_channel_invite(&self, ctx: &Context<'_>, id: ID) -> FieldResult<bool> {
        let chat = chat_state(ctx)?;
        Ok(chat.service.decline_channel_invite(&id.to_string()).await)
    }

    /// Web-only convenience (plain-app Android doesn't expose it): branch
    /// to accept/decline on the `accept` flag, returned verbatim.
    async fn respond_channel_invite(
        &self,
        ctx: &Context<'_>,
        id: ID,
        accept: bool,
    ) -> FieldResult<bool> {
        let chat = chat_state(ctx)?;
        Ok(chat
            .service
            .respond_channel_invite(&id.to_string(), accept)
            .await)
    }

    /// Debug-only stub (plain-desktop parity): the peer protocol invokes
    /// `channelSystemMessage` via `/peer_graphql`, never the main schema —
    /// exposing a real implementation here would double-process payloads.
    async fn channel_system_message(
        &self,
        #[graphql(name = "type")] _msg_type: String,
        _payload: String,
    ) -> bool {
        false
    }

    /// Send a chat message. `target` is a bare/`peer:` peer id or
    /// `channel:<id>`; `content` is the message envelope JSON.
    async fn send_chat_item(
        &self,
        ctx: &Context<'_>,
        #[graphql(name = "target")] target: String,
        content: String,
    ) -> FieldResult<Vec<ChatItem>> {
        let chat = chat_state(ctx)?;
        Ok(chat
            .service
            .send_chat_item(target, content)
            .iter()
            .map(|c| crate::api::schema::nas::types::chat_item_from_dchat(c))
            .collect())
    }

    async fn delete_chat_item(&self, ctx: &Context<'_>, id: ID) -> FieldResult<bool> {
        let chat = chat_state(ctx)?;
        Ok(chat.service.delete_chat_item(id.to_string()))
    }

    /// Bulk delete by query DSL (`ids:` / `channel:` / `peer:`) — blank
    /// query is a no-op returning `affectedCount: 0` (§5 exemption).
    async fn delete_chat_items(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> FieldResult<ActionResult> {
        let chat = chat_state(ctx)?;
        let n = chat.service.delete_chat_items(query);
        Ok(ActionResult { affected_count: n })
    }

    async fn retry_chat_item(&self, ctx: &Context<'_>, id: ID) -> FieldResult<ChatItem> {
        let chat = chat_state(ctx)?;
        chat.service
            .retry_chat_item(id.to_string())
            .map(|c| crate::api::schema::nas::types::chat_item_from_dchat(&c))
            .ok_or_else(|| async_graphql::Error::new("chat item not found"))
    }

    /// Delete a peer: 1:1 chats removed; a peer still inside a channel is
    /// demoted to CHANNEL status instead of being dropped.
    async fn delete_peer(&self, ctx: &Context<'_>, id: ID) -> FieldResult<bool> {
        let chat = chat_state(ctx)?;
        Ok(chat.service.delete_peer(&id.to_string()))
    }

    /// Mark a peer UNPAIRED (shared key kept for a future re-pair).
    async fn unpair_peer(&self, ctx: &Context<'_>, id: ID) -> FieldResult<bool> {
        let chat = chat_state(ctx)?;
        Ok(chat.service.unpair_peer(&id.to_string()))
    }

    /// Initiate pairing with a discovered LAN device — sends a signed
    /// PAIR_REQUEST to the target's `POST /nearby` endpoint.
    async fn pair_device(
        &self,
        ctx: &Context<'_>,
        #[graphql(name = "input")] input: PairingDeviceInput,
    ) -> FieldResult<bool> {
        let chat = chat_state(ctx)?;
        let ip = input
            .ips
            .first()
            .cloned()
            .ok_or_else(|| async_graphql::Error::new("pairing target has no ip"))?;
        let local_port = pairing_local_port(ctx)?;
        chat.pairing.start_pairing(
            &input.id.to_string(),
            &input.name,
            &ip,
            input.port as u16,
            local_port,
        );
        Ok(true)
    }

    /// Cancel an in-progress pairing initiated by this device.
    async fn cancel_pairing(&self, ctx: &Context<'_>, device_id: ID) -> FieldResult<bool> {
        let chat = chat_state(ctx)?;
        chat.pairing.cancel_pairing(&device_id.to_string());
        Ok(true)
    }

    /// Respond to an incoming pairing request — accept or reject.
    async fn respond_to_pairing(
        &self,
        ctx: &Context<'_>,
        #[graphql(name = "input")] input: PairingRequestInput,
        accepted: bool,
    ) -> FieldResult<bool> {
        let chat = chat_state(ctx)?;
        let local_port = pairing_local_port(ctx)?;
        let request = crate::api::schema::nas::types::pairing_request_from_input(&input);
        chat.pairing
            .respond_to_pairing(request, &input.from_ip, accepted, local_port);
        Ok(true)
    }

    // ----- Chunked upload -----

    /// Same merge, but the result is imported into the content-addressed
    /// app-file store (dedup); the returned `value` is the fid suffix
    /// (`"{hash}.{ext}"`) the client builds `fid:` URIs from.
    async fn merge_app_file_chunks(
        &self,
        ctx: &Context<'_>,
        #[graphql(name = "fileId")] file_id: String,
        total_chunks: i32,
        #[graphql(name = "fileName")] file_name: String,
        #[graphql(name = "totalSize")] _total_size: Long,
    ) -> FieldResult<MergeTask> {
        // Idempotent re-calls: terminal states reply immediately, an
        // in-flight job reports MERGING.
        {
            let mut jobs = merge_jobs().lock().unwrap();
            match jobs.get(file_id.as_str()) {
                Some(MergeJobState::Done { value, merged_size }) => {
                    return Ok(MergeTask {
                        status: MergeTaskStatus::DONE,
                        value: Some(value.clone()),
                        merged_size: Some(Long(*merged_size)),
                        error: None,
                    });
                }
                Some(MergeJobState::Merging) => return Ok(merge_job_status(&file_id)),
                _ => {}
            }
            jobs.insert(file_id.to_string(), MergeJobState::Merging);
        }
        let chat = chat_state(ctx)?;
        let paths = crate::nas::consts::AppPaths::detect();
        let data_dir = paths.data_dir.clone();
        let db = chat.service.db.clone();
        let fid = file_id.to_string();
        tokio::spawn(async move {
            // Merge to a temp path OUTSIDE the chunk dir — a successful
            // merge removes the chunk dir, and the temp file must survive
            // until import_file has copied it into the store.
            let temp = crate::nas::chunked_upload::chunk_dir(&data_dir, &fid)
                .parent()
                .map(|p| p.join(format!(".merge_app_{fid}")))
                .unwrap_or_else(|| data_dir.join(format!(".merge_app_{fid}")));
            let temp_str = temp.to_string_lossy().to_string();
            let temp_path = temp.clone();
            let merged = crate::nas::chunked_upload::merge_chunks(
                &data_dir,
                &fid,
                total_chunks,
                &temp_str,
                true,
            )
            .await;
            let outcome = match merged {
                Err(e) => Err(e),
                Ok((_value, merged_size)) => {
                    // Import into the content-addressed store (blocking
                    // work: hashing + copy).
                    let import_data_dir = data_dir.clone();
                    let imported = tokio::task::spawn_blocking(move || {
                        crate::chat::app_file_store::import_file(
                            &db,
                            &import_data_dir,
                            &temp_path,
                            &file_name,
                            "",
                        )
                    })
                    .await
                    .map_err(|e| anyhow::anyhow!("import task failed: {e}"))
                    .and_then(|r| r.map_err(|e| anyhow::anyhow!("import failed: {e}")));
                    match imported {
                        Ok(res) => {
                            let _ = std::fs::remove_file(
                                crate::nas::chunked_upload::chunk_dir(&data_dir, &fid)
                                    .parent()
                                    .map(|p| p.join(format!(".merge_app_{fid}")))
                                    .unwrap_or_else(|| {
                                        data_dir.clone().join(format!(".merge_app_{fid}"))
                                    }),
                            );
                            Ok((res.fid_suffix, merged_size))
                        }
                        Err(e) => Err(e),
                    }
                }
            };
            let mut jobs = merge_jobs().lock().unwrap();
            match outcome {
                Ok((value, merged_size)) => {
                    jobs.insert(
                        fid.clone(),
                        MergeJobState::Done {
                            value,
                            merged_size: merged_size as i64,
                        },
                    );
                }
                Err(e) => {
                    jobs.insert(fid.clone(), MergeJobState::Failed(e.to_string()));
                }
            }
        });
        Ok(MergeTask {
            status: MergeTaskStatus::STARTED,
            value: None,
            merged_size: None,
            error: None,
        })
    }

    /// Start a background merge (plain-app contract): the mutation returns
    /// immediately with STARTED (or DONE/MERGING for idempotent re-calls);
    /// completion is observable via `mergeStatus` polling.
    async fn merge_chunks(
        &self,
        _ctx: &Context<'_>,
        #[graphql(name = "fileId")] file_id: String,
        total_chunks: i32,
        path: String,
        replace: bool,
        #[graphql(name = "totalSize")] _total_size: Long,
    ) -> FieldResult<MergeTask> {
        // Idempotent re-calls: terminal states reply immediately, an
        // in-flight job reports MERGING.
        {
            let mut jobs = merge_jobs().lock().unwrap();
            match jobs.get(file_id.as_str()) {
                Some(MergeJobState::Done { value, merged_size }) => {
                    return Ok(MergeTask {
                        status: MergeTaskStatus::DONE,
                        value: Some(value.clone()),
                        merged_size: Some(Long(*merged_size)),
                        error: None,
                    });
                }
                Some(MergeJobState::Merging) => {
                    return Ok(merge_job_status(&file_id));
                }
                _ => {}
            }
            jobs.insert(file_id.to_string(), MergeJobState::Merging);
        }
        let paths = crate::nas::consts::AppPaths::detect();
        let data_dir = paths.data_dir.clone();
        let fid = file_id.to_string();
        tokio::spawn(async move {
            let outcome = crate::nas::chunked_upload::merge_chunks(
                &data_dir,
                &fid,
                total_chunks,
                &path,
                replace,
            )
            .await;
            let mut jobs = merge_jobs().lock().unwrap();
            match outcome {
                Ok((value, merged_size)) => {
                    jobs.insert(
                        fid.clone(),
                        MergeJobState::Done {
                            value,
                            merged_size: merged_size as i64,
                        },
                    );
                }
                Err(e) => {
                    jobs.insert(fid.clone(), MergeJobState::Failed(e.to_string()));
                }
            }
        });
        Ok(MergeTask {
            status: MergeTaskStatus::STARTED,
            value: None,
            merged_size: None,
            error: None,
        })
    }

    // ----- Media scan -----

    // ----- Disk format -----

    /// Wipe the whole disk at `path` and create a single GPT + ext4 partition (label `plainnas`): unmounts first (automount inhibited for the duration), then `wipefs`/`sfdisk`/`mkfs.ext4`, then re-coordinates the automount slot. Synchronous — returns after formatting finished; failures raise and are audited (`FORMAT_DISK_FAILED`).
    async fn format_disk(&self, ctx: &Context<'_>, path: String) -> FieldResult<bool> {
        let cid: String = ctx.data::<String>()?.clone();
        let prefs = ctx.data::<std::sync::Arc<crate::prefs::Prefs>>()?.clone();
        let on_unmount = |mp: &str| {
            let _ = crate::media::kv::EventLog::new(crate::media::kv::get_default())
                .add("unmount", mp, &cid);
        };
        // Broadcast the outcome so every connected client (not just the
        // caller) can refresh mounts without polling. msg_type 45.
        let publish_done = |ok: bool, err: Option<String>| {
            // Publish returns () — best-effort broadcast, nothing to handle.
            crate::media::eventbus::Bus::new().publish(
                crate::media::eventbus::EVENT_DISK_FORMAT_DONE,
                serde_json::json!({ "path": path, "ok": ok, "error": err }),
            );
        };
        match crate::nas::format_disk::format_disk_single_partition(&prefs, &path, on_unmount) {
            Ok(()) => {
                publish_done(true, None);
                let _ = crate::media::kv::EventLog::new(crate::media::kv::get_default()).add(
                    "format_disk",
                    &path,
                    &cid,
                );
                Ok(true)
            }
            Err(e) => {
                publish_done(false, Some(e.to_string()));
                let _ = crate::media::kv::EventLog::new(crate::media::kv::get_default()).add(
                    "format_disk_failed",
                    &format!("{path}: {e}"),
                    &cid,
                );
                Err(async_graphql::Error::new(e.to_string()))
            }
        }
    }

    // ----- Mount alias -----

    /// Persist a user-defined alias for a storage volume. Mirrors Go
    /// `SetMountAlias` (which delegates to `db.SetVolumeAlias`).
    async fn set_mount_alias(&self, ctx: &Context<'_>, id: ID, alias: String) -> FieldResult<bool> {
        let prefs = ctx.data::<std::sync::Arc<crate::prefs::Prefs>>()?;
        // The mount id is the `fsuuid:`/`dev:`/`remote:` composite — the
        // same value space as `StorageMount.id`, carried as ID per §4.
        crate::media::kv::storage::set_alias(prefs, id.as_str(), &alias)?;
        Ok(true)
    }

    // ----- Media source dirs -----

    // ----- setTempValue -----

    /// Persist a short-lived key/value pair for transient UI state —
    /// plain-app `setTempValue` (its `TempHelper` is in-memory; ours is
    /// too, consumed by `/zip?tmp=<key>`).
    async fn set_temp_value(
        &self,
        _ctx: &Context<'_>,
        key: String,
        value: String,
    ) -> FieldResult<KeyValuePair> {
        if key.trim().is_empty() {
            return Err(async_graphql::Error::new("key is empty"));
        }
        crate::nas::temp_store::set(&key, &value);
        Ok(KeyValuePair { key, value })
    }

    // ----- Samba -----

    /// Mirrors Go `setSambaSettings`: validate against the previous
    /// settings (password shares require a provisioned password), persist,
    /// then apply to the system (smb.conf + service).
    async fn set_samba_settings(
        &self,
        ctx: &Context<'_>,
        input: SambaSettingsInput,
    ) -> FieldResult<bool> {
        let prefs = ctx.data::<std::sync::Arc<crate::prefs::Prefs>>()?;
        let prev = crate::nas::samba::get_samba_settings(prefs);

        let mut requires_password = false;
        let shares = input
            .shares
            .into_iter()
            .map(|s| {
                let auth = match s.auth {
                    SambaShareAuth::GUEST => crate::nas::samba::SambaShareAuth::Guest,
                    SambaShareAuth::PASSWORD => {
                        requires_password = true;
                        crate::nas::samba::SambaShareAuth::Password
                    }
                };
                crate::nas::samba::SambaShare {
                    name: s.name,
                    share_path: s.share_path,
                    auth,
                    read_only: s.read_only,
                }
            })
            .collect();
        let mut desired = prev.clone();
        desired.enabled = input.enabled;
        desired.shares = shares;

        if desired.enabled && desired.shares.is_empty() {
            return Err(async_graphql::Error::new("no shares configured"));
        }
        if requires_password && !prev.has_password {
            return Err(async_graphql::Error::new("password required"));
        }

        crate::nas::samba::set_samba_settings(prefs, &desired)
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        // Apply the normalized stored settings.
        let desired = crate::nas::samba::get_samba_settings(prefs);
        crate::nas::samba::apply(prefs, &desired, "")
            .map(|_| ())
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(true)
    }

    /// Mirrors Go `setSambaUserPassword`: provision the samba password,
    /// persist `hasPassword`, and re-apply when samba is enabled.
    async fn set_samba_user_password(
        &self,
        ctx: &Context<'_>,
        password: String,
    ) -> FieldResult<bool> {
        let prefs = ctx.data::<std::sync::Arc<crate::prefs::Prefs>>()?;
        if password.trim().is_empty() {
            return Err(async_graphql::Error::new("password required"));
        }
        crate::nas::samba::set_user_password(&password)
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        let mut s = crate::nas::samba::get_samba_settings(prefs);
        s.has_password = true;
        crate::nas::samba::set_samba_settings(prefs, &s)
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;

        if s.enabled {
            let _ = crate::nas::samba::apply(prefs, &s, "");
        }
        Ok(true)
    }

    // ----- Media item bulk actions -----
    //
    // These mirror the phone `trashMediaItems` / `restoreMediaItems` /
    // `deleteMediaItems` mutations and return the shared `ActionResult`
    // (affectedCount = matching items actually processed).

    // ----- DLNA -----

    /// Push `url` to the DLNA renderer with the given UDN (SOAP AVTransport, 3s timeout). `type` selects the DIDL media class; DOC is rejected.
    async fn dlna_cast(
        &self,
        ctx: &Context<'_>,
        renderer_udn: String,
        url: String,
        title: String,
        mime: String,
        #[graphql(name = "type")] media_type: MediaDataType,
    ) -> FieldResult<bool> {
        // Mirrors Go `dlnaCastModel`: 3s timeout wrapping the SOAP calls,
        // then a `MediaDataType` → `dlna.MediaType` mapping. Go accepted the
        // wider `DataType` and silently defaulted unknown kinds to Video;
        // the media-only enum makes that fallback impossible, so DOC is an
        // explicit error instead of a nonsense cast.
        let mt = match media_type {
            MediaDataType::AUDIO => crate::nas::dlna::MediaType::Audio,
            MediaDataType::VIDEO => crate::nas::dlna::MediaType::Video,
            MediaDataType::IMAGE => crate::nas::dlna::MediaType::Image,
            MediaDataType::DOC => {
                return Err(async_graphql::Error::new("dlna_cast_doc_unsupported"));
            }
        };
        // Clone the Arc so the spawn_blocking task can resolve encrypted
        // file ids inside `dlna::cast` via `fsx::path_from_file_id`.
        let prefs = ctx.data::<std::sync::Arc<crate::prefs::Prefs>>()?.clone();
        match tokio::task::spawn_blocking(move || {
            crate::nas::dlna::cast(&renderer_udn, &url, &title, &mime, mt, &prefs)
        })
        .await
        {
            Ok(Ok(())) => Ok(true),
            Ok(Err(e)) => Err(async_graphql::Error::new(e)),
            Err(e) => Err(async_graphql::Error::new(format!("join error: {e}"))),
        }
    }
}

// ----- helpers shared by the two roots -----

pub fn favorite_to_gql(f: crate::library::favorite_folders::FavoriteFolder) -> FavoriteFolder {
    FavoriteFolder {
        full_path: crate::library::favorite_folders::full_path_of(&f),
        root_path: f.root_path,
        relative_path: f.relative_path,
        alias: f.alias,
    }
}

/// The full favorite list as GraphQL objects (phone contract: the favorite
/// folder mutations return the updated list, not the touched entry).
fn favorite_list_gql(library: &crate::library::db::LibraryDb) -> Vec<FavoriteFolder> {
    crate::library::favorite_folders::list(library)
        .into_iter()
        .map(favorite_to_gql)
        .collect()
}

pub fn bookmark_to_gql(b: crate::chat::db::bookmark::DBookmark) -> Bookmark {
    Bookmark {
        id: b.id.into(),
        url: b.url,
        title: b.title,
        favicon_path: b.favicon_path,
        group_id: b.group_id.into(),
        pinned: b.pinned,
        click_count: b.click_count,
        last_clicked_at: b.last_clicked_at.as_deref().map(instant_of),
        sort_order: b.sort_order,
        created_at: instant_of(&b.created_at),
        updated_at: instant_of(&b.updated_at),
    }
}

/// Parse an ISO-8601 row timestamp (plain-rs dbtime format) into the
/// GraphQL `Instant` scalar; unparseable input degrades to the epoch
/// rather than failing the response.
pub(crate) fn instant_of(iso: &str) -> Instant {
    chrono::DateTime::parse_from_rfc3339(iso)
        .map(|d| Instant(d.with_timezone(&chrono::Utc)))
        .unwrap_or_default()
}

pub fn bookmark_group_to_gql(
    g: crate::chat::db::bookmark::DBookmarkGroup,
    item_count: usize,
) -> BookmarkGroup {
    BookmarkGroup {
        id: g.id.into(),
        name: g.name,
        collapsed: g.collapsed,
        sort_order: g.sort_order,
        item_count: item_count.min(i32::MAX as usize) as i32,
        created_at: instant_of(&g.created_at),
        updated_at: instant_of(&g.updated_at),
    }
}

/// Last path segment, mirroring Kotlin `File.name` ("/" and "" → "").

/// Mirrors Go `mediaTypeFromDataType` (`DataType` enum → numeric kind).
/// Values follow plain-app `DataType.value` exactly.

/// `DataType` → media index type filter ("audio" / "video" / "image" / "doc").

/// The explicit `ids:a,b,c` DSL group from a tag-mutation query (checkbox
/// selections) → the media keys to tag. `None` when the query has no ids.
fn parse_ids_query(query: &str) -> Option<Vec<String>> {
    let fields = crate::media::search::parse(query);
    let mut ids = String::new();
    for f in &fields {
        if f.name == "ids" {
            ids = f.value.clone();
        }
    }
    if ids.trim().is_empty() {
        return None;
    }
    Some(
        ids.split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

/// Resolve an audio-page query (`ids:a,b,c` for checkbox selections,
/// otherwise the shared search DSL) into queue tracks — mirrors plain-app
/// `searchMedia(AUDIO, query, limit, 0, sortBy)` feeding `enqueue`.
fn resolve_queue_tracks(
    db: &std::sync::Arc<crate::media::kv::Db>,
    query: &str,
    limit: usize,
) -> Vec<crate::library::audio_queue::AudioTrack> {
    let to_track = |title: String, name: String, artist: String, path: String, secs: i64| {
        crate::library::audio_queue::AudioTrack {
            title: if title.is_empty() { name } else { title },
            artist,
            path,
            duration_secs: secs,
        }
    };
    if let Some(ids) = parse_ids_query(query) {
        return ids
            .iter()
            .filter_map(|id| crate::media::scan::get_by_uuid(db, id).ok().flatten())
            .map(|mut m| {
                // Enqueue-time hydration: the queue row stores what the
                // player shows, so fill in anything the index lacks first.
                crate::media::scan::hydrate_metadata(db, &mut m);
                to_track(m.title, m.name, m.artist, m.path, m.duration_sec as i64)
            })
            .collect();
    }
    match crate::media::image_index::global().search(
        query,
        Some("audio"),
        None,
        crate::media::image_index::MediaSort::DateDesc,
        0,
        limit,
    ) {
        Ok(mut rows) => {
            crate::media::scan::hydrate_search_page(db, &mut rows, true);
            rows.into_iter()
                .map(|r| to_track(r.title, r.name, r.artist, r.path, r.duration_secs as i64))
                .collect()
        }
        Err(e) => {
            log::error!("[audio-queue] resolve tracks failed for {query:?}: {e}");
            Vec::new()
        }
    }
}

/// Resolve a tag-mutation `query` (the shared search DSL: `ids:a,b,c` for
/// Last path segment, mirroring Kotlin `File.name` ("/" and "" → "").
pub fn file_name_of(path: &str) -> String {
    std::path::Path::new(path.trim_end_matches('/'))
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string()
}

/// Same rules as Go `sanitizeHostname`: lowercase, `[a-z0-9-]` only, no
/// consecutive dashes, no leading/trailing dash, length 1..=63.
fn sanitize_hostname(input: &str) -> String {
    let v = input
        .trim()
        .to_lowercase()
        .replace('_', "-")
        .replace(' ', "-")
        .replace('.', "-");
    let mut out = String::with_capacity(v.len());
    let mut prev_dash = false;
    for ch in v.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_dash = false;
        } else if ch == '-' && !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.len() > 63 {
        out.truncate(63);
    }
    out
}

#[cfg(test)]
#[path = "../../../../tests/unit/api/schema/nas/mutation.rs"]
mod tests;
