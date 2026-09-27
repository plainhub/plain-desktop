//! `Query` root. The Go schema declares ~30 fields; we ship the minimum set
//! needed for the UI to load and present the home page. Higher-tier fields
//! raise "not implemented" errors so the frontend can gracefully degrade.

use super::types::*;
use async_graphql::{Context, FieldResult, Object};
use std::sync::Arc;

pub struct NasQueryRoot {
    pub db: Arc<crate::media::kv::Db>,
    pub prefs: Arc<crate::prefs::Prefs>,
    pub config: Arc<crate::media::config::Config>,
    /// Data dir (PLAIN_NAS_DATA_DIR or /var/lib/plainnas) — root for the
    /// on-disk log (`appLogPath`), the fjall store (`dbPath`) and the
    /// preferences file (`dataStorePath`).
    pub data_dir: std::path::PathBuf,
}

// `App.permissions`: the NAS serves storage-backed media (images, videos,
// audios, docs, files) from its own always-accessible disks, so it
// declares WRITE_EXTERNAL_STORAGE — the permission the web media pages'
// empty states gate on. Phone-only permissions have no NAS data source
// and stay unreported.

impl NasQueryRoot {
    pub fn new(
        db: Arc<crate::media::kv::Db>,
        prefs: Arc<crate::prefs::Prefs>,
        config: Arc<crate::media::config::Config>,
        data_dir: std::path::PathBuf,
    ) -> Self {
        Self {
            db,
            prefs,
            config,
            data_dir,
        }
    }
}

/// Capability flags served by the `app` query. The web client gates
/// feature entries on `app.features` alone (never on `deviceType`), so
/// every optional capability must be declared here. Pure over the host
/// probes so tests can lock the mapping without touching the machine.
fn declared_capabilities(
    samba_unit_loaded: bool,
    lsblk_present: bool,
    doc_preview_present: bool,
) -> Vec<Capability> {
    let mut caps = vec![Capability::MEDIA_TRASH, Capability::MEDIA_SCAN];
    if samba_unit_loaded {
        caps.push(Capability::LAN_SHARE);
    }
    if lsblk_present {
        caps.push(Capability::DISK_MANAGER);
    }
    if doc_preview_present {
        caps.push(Capability::DOC_PREVIEW);
    }
    caps
}

#[Object]
impl NasQueryRoot {
    /// Top-level `app` info used by the web client on startup.
    async fn app(&self, _ctx: &Context<'_>) -> FieldResult<App> {
        let url_token = crate::media::kv::UrlToken::new(&self.prefs)
            .ensure()
            .unwrap_or_default();
        let http_port = self.config.get_int("server.http_port") as i32;
        let https_port = self.config.get_int("server.https_port") as i32;
        let hostname = match crate::nas::device_info::collect().await {
            Ok(info) => info.hostname,
            Err(_) => String::from("plainnas"),
        };
        // plain-app semantics: the stored display name wins; empty falls
        // back to the system name (`.ifEmpty { getDeviceName() }`).
        let stored_name = crate::media::kv::device_display_name(&self.prefs);
        let device_name = if stored_name.is_empty() {
            hostname
        } else {
            stored_name
        };
        Ok(App {
            client_id: crate::media::kv::server_client_id(&self.prefs),
            url_token,
            http_port,
            https_port,
            // A NAS's "app dir" is its data dir (fjall store, prefs.json,
            // on-disk log all live here).
            app_dir: self.data_dir.to_string_lossy().to_string(),
            device_name,
            device_type: DeviceType::NAS,
            // Filesystem-level media trash is always implemented
            // (`.nas-trash`), as is the media scan/index engine the web
            // home ScanPanel drives; the rest of the capabilities are
            // host probes via `declared_capabilities`. A NAS has no
            // screen-mirror audio source.
            capabilities: declared_capabilities(
                !crate::nas::samba::detect_systemd_service_name().is_empty(),
                which("lsblk").is_some(),
                which("libreoffice").is_some() || which("soffice").is_some(),
            ),
            build_channel: AppChannelType::GITHUB,
            permissions: vec![Permission::WRITE_EXTERNAL_STORAGE],
            downloads_dir: String::new(),
            developer_mode: false,
            debug: cfg!(debug_assertions),
        })
    }

    /// Chunked-upload merge job state (plain-app `mergeStatus` polling).
    async fn merge_status(&self, #[graphql(name = "fileId")] file_id: String) -> MergeTask {
        super::mutation::merge_job_status_pub(&file_id)
    }

    /// Player state (plain-app `AudioPlayback`): play mode preference and
    /// the current queue track path (null = idle). The NAS has no
    /// server-side transport — audio renders on the client — so
    /// isPlaying/positionMs serve idle values (API_SPEC §9).
    async fn audio_playback(&self, ctx: &Context<'_>) -> FieldResult<AudioPlayback> {
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        let current = crate::library::audio_queue::get_audio_current(library);
        Ok(AudioPlayback {
            current_path: (!current.is_empty()).then_some(current),
            mode: media_play_mode_of(library),
            is_playing: false,
            position_ms: Long(0),
        })
    }

    /// Chat channels the device has not left (plain-app `chatChannels`).
    async fn chat_channels(&self, ctx: &Context<'_>) -> FieldResult<Vec<ChatChannel>> {
        let chat = ctx.data::<std::sync::Arc<crate::api::chat::ChatState>>()?;
        Ok(chat
            .service
            .db
            .get_channels(crate::chat::enums::ChannelStatus::Joined)
            .into_iter()
            .map(crate::api::schema::nas::types::chat_channel_from_dchannel)
            .collect())
    }

    /// Chat items of one conversation. Paginated latest-first like
    /// plain-app; returns oldest-to-newest. `target` is a bare/`peer:`
    /// peer id or a `channel:<id>` prefixed channel id; `query` is the
    /// shared DSL of which only the `text:` field applies.
    async fn chat_items(
        &self,
        ctx: &Context<'_>,
        #[graphql(name = "target")] target: String,
        offset: i32,
        limit: i32,
        query: String,
    ) -> FieldResult<Vec<ChatItem>> {
        let chat = ctx.data::<std::sync::Arc<crate::api::chat::ChatState>>()?;
        let text = text_of(&query);
        Ok(chat
            .service
            .db
            .get_chats_page(&target, &text, offset, limit)
            .iter()
            .map(|c| crate::api::schema::nas::types::chat_item_from_dchat(c))
            .collect())
    }

    /// Latest items across conversations (one per channel/peer/local).
    async fn latest_chat_items(&self, ctx: &Context<'_>) -> FieldResult<Vec<ChatItem>> {
        let chat = ctx.data::<std::sync::Arc<crate::api::chat::ChatState>>()?;
        Ok(chat
            .service
            .db
            .get_all_latest_chats()
            .iter()
            .map(|c| crate::api::schema::nas::types::chat_item_from_dchat(c))
            .collect())
    }

    /// Paired / known LAN peers. `online` reflects live reachability
    /// (mDNS presence); unknown peers report `false`.
    async fn peers(&self, ctx: &Context<'_>) -> FieldResult<Vec<Peer>> {
        let chat = ctx.data::<std::sync::Arc<crate::api::chat::ChatState>>()?;
        Ok(chat
            .service
            .db
            .get_peers()
            .into_iter()
            .map(|p| {
                let online = chat.discovery.as_ref().is_some_and(|d| d.is_online(&p.id));
                crate::api::schema::nas::types::peer_from_dpeer(p, online)
            })
            .collect())
    }

    /// Chat attachment store page (newest first), filtered by display
    /// name substring.
    async fn app_files(
        &self,
        ctx: &Context<'_>,
        offset: i32,
        limit: i32,
        query: String,
    ) -> FieldResult<Vec<AppFile>> {
        let chat = ctx.data::<std::sync::Arc<crate::api::chat::ChatState>>()?;
        let text = text_of(&query);
        let name_map = crate::chat::app_file_store::file_name_map(&chat.service.db.get_all_chats());
        Ok(chat
            .service
            .db
            .get_app_file_page(limit, offset)
            .iter()
            .map(|f| {
                let display = crate::chat::app_file_store::display_name(f, &name_map);
                crate::api::schema::nas::types::app_file_from_dappfile(f, display)
            })
            .filter(|f| text.is_empty() || f.file_name.contains(&text))
            .collect())
    }

    /// Chat attachment count, same filter as `appFiles`.
    async fn app_file_count(&self, ctx: &Context<'_>, query: String) -> FieldResult<i32> {
        let chat = ctx.data::<std::sync::Arc<crate::api::chat::ChatState>>()?;
        let text = text_of(&query);
        if text.is_empty() {
            return Ok(chat.service.db.count_app_files());
        }
        let name_map = crate::chat::app_file_store::file_name_map(&chat.service.db.get_all_chats());
        Ok(chat
            .service
            .db
            .get_all_app_files()
            .iter()
            .filter(|f| {
                let display = crate::chat::app_file_store::display_name(f, &name_map);
                display.contains(&text)
            })
            .count() as i32)
    }

    /// All bookmarks, phone-DAO order (pinned first, then sort order).
    async fn bookmarks(&self, ctx: &Context<'_>) -> FieldResult<Vec<Bookmark>> {
        let chat = ctx.data::<std::sync::Arc<crate::api::chat::ChatState>>()?;
        Ok(crate::chat::db::bookmark::get_bookmarks(&chat.service.db)
            .into_iter()
            .map(super::mutation::bookmark_to_gql)
            .collect())
    }

    /// All bookmark groups, phone-DAO order (sort order, then creation).
    /// `itemCount` is live — one prefix scan for the whole list.
    async fn bookmark_groups(&self, ctx: &Context<'_>) -> FieldResult<Vec<BookmarkGroup>> {
        let chat = ctx.data::<std::sync::Arc<crate::api::chat::ChatState>>()?;
        let db = &chat.service.db;
        let bookmarks = crate::chat::db::bookmark::get_bookmarks(db);
        Ok(crate::chat::db::bookmark::get_bookmark_groups(db)
            .into_iter()
            .map(|g| {
                let item_count = bookmarks.iter().filter(|b| b.group_id == g.id).count();
                super::mutation::bookmark_group_to_gql(g, item_count)
            })
            .collect())
    }

    /// The active playback queue (manual items + context), paginated —
    /// plain-app `AudioQueueManager.queuePage`, never materialized whole.
    /// `query` is the shared DSL; its `text:` field filters the page
    /// (case-insensitive substring over title/artist/path).
    async fn audio_queue_items(
        &self,
        ctx: &Context<'_>,
        offset: i32,
        limit: i32,
        query: String,
    ) -> FieldResult<Vec<AudioItem>> {
        let text = text_of(&query);
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        let library = (**library).clone();
        let db = self.db.clone();
        let index = crate::media::image_index::global();
        let items = super::run_blocking(move || {
            let mut tracks = crate::nas::library::NasLibraryTracks::new(db, index);
            crate::library::audio_queue::queue_page(
                &library,
                &mut tracks,
                offset as i64,
                limit as i64,
                &text,
            )
            .map_err(|e| async_graphql::Error::new(e.to_string()))
        })
        .await??;
        Ok(items.into_iter().map(playlist_audio_to_gql).collect())
    }

    /// Total tracks in the active playback queue (superseded source copies
    /// excluded).
    async fn audio_queue_item_count(&self, ctx: &Context<'_>) -> FieldResult<i32> {
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        let library = (*library).clone();
        let db = self.db.clone();
        let index = crate::media::image_index::global();
        let n = super::run_blocking(move || {
            let mut tracks = crate::nas::library::NasLibraryTracks::new(db, index);
            crate::library::audio_queue::queue_total(&library, &mut tracks)
                .map_err(|e| async_graphql::Error::new(e.to_string()))
        })
        .await??;
        Ok(n.min(i32::MAX as usize) as i32)
    }

    /// Lyrics embedded in the audio file (ID3v2 USLT, Vorbis comments,
    /// MP4 ©lyr); empty string when none.
    async fn audio_lyrics(&self, _ctx: &Context<'_>, path: String) -> FieldResult<String> {
        let lyrics = tokio::task::spawn_blocking(move || {
            crate::media::lyrics::extract_lyrics_from_path(&path)
        })
        .await
        .map_err(|e| async_graphql::Error::new(format!("join: {e}")))?;
        // Non-null contract (2026-09-22 user decision): no lyrics = empty
        // string, so clients never branch on null.
        Ok(lyrics)
    }

    /// All user playlists, most recently updated first.
    async fn audio_playlists(&self, ctx: &Context<'_>) -> FieldResult<Vec<AudioPlaylist>> {
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        let rows = crate::library::audio_queue::playlists(library);
        Ok(rows
            .into_iter()
            .map(|(pl, count)| AudioPlaylist {
                id: pl.id.into(),
                name: pl.name,
                item_count: count as i32,
                created_at: super::mutation::instant_of(&pl.created_at),
                updated_at: super::mutation::instant_of(&pl.updated_at),
            })
            .collect())
    }

    /// One playlist's tracks, position order, paginated. `query` is the
    /// shared DSL; its `text:` field filters the page (case-insensitive
    /// substring over title/artist/path).
    async fn audio_playlist_items(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        offset: i32,
        limit: i32,
        query: String,
    ) -> FieldResult<Vec<AudioItem>> {
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        let items = crate::library::audio_queue::playlist_items_page(
            library,
            &id,
            offset as i64,
            limit as i64,
            text_of(&query).as_str(),
        );
        Ok(items.into_iter().map(playlist_audio_to_gql).collect())
    }

    /// Track count of one playlist.
    async fn audio_playlist_item_count(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> FieldResult<i32> {
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        Ok(
            crate::library::audio_queue::playlist_item_count(library, &id).min(i32::MAX as usize)
                as i32,
        )
    }

    /// Recently played tracks, newest first (the home "最近" section).
    /// `query` is the shared DSL; its `text:` field filters the page
    /// (case-insensitive substring over title/artist/path).
    async fn audio_play_history(
        &self,
        ctx: &Context<'_>,
        offset: i32,
        limit: i32,
        query: String,
    ) -> FieldResult<Vec<AudioPlayHistory>> {
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        let rows = crate::library::audio_queue::history_page(
            library,
            offset as i64,
            limit as i64,
            text_of(&query).as_str(),
        );
        Ok(rows
            .into_iter()
            .map(|h| AudioPlayHistory {
                path: h.path,
                title: h.title,
                artist: h.artist,
                duration_ms: Long(h.duration_secs as i64 * 1000),
                play_count: Long(h.play_count),
                played_at: super::mutation::instant_of(&h.played_at),
            })
            .collect())
    }

    /// System/device metadata, plain-app `DeviceInfo` contract. Phone-only
    /// sub-objects (`android`, `display`) are null; static identity/spec —
    /// dynamic state lives in `deviceStatus`.
    async fn device_info(&self) -> FieldResult<DeviceInfo> {
        let info = crate::nas::device_info::collect().await?;
        // plain-app `DeviceInfo.name` is the device display name; same
        // precedence as `App.deviceName` (stored override, else hostname).
        let stored_name = crate::media::kv::device_display_name(&self.prefs);
        let name = if stored_name.is_empty() {
            info.hostname.clone()
        } else {
            stored_name
        };
        // "Device storage" for a NAS = the root filesystem's capacity.
        let total_storage = crate::nas::mounts::df_usage("/").0;
        Ok(DeviceInfo {
            name,
            platform: DevicePlatform::LINUX,
            // /sys/class/dmi/id reports the real board (NanoPi R5S, …).
            manufacturer: crate::nas::device_info::dmi_manufacturer(),
            model: info.model,
            os_name: String::from("Linux"),
            os_version: info.os,
            kernel_version: info.kernel_version,
            app_version: info.app_version,
            app_build_number: String::new(),
            language: String::new(),
            cpu_arch: info.arch,
            cpu_model: (!info.cpu_model.is_empty()).then_some(info.cpu_model),
            total_memory: Long(info.memory_total_bytes),
            total_storage: Long(total_storage),
            display: None,
            android: None,
        })
    }

    /// Dynamic device runtime state (plain-app `DeviceStatus`). CPU usage
    /// is diffed from two /proc/stat samples taken 200ms apart, so this
    /// resolver deliberately runs on the blocking pool.
    async fn device_status(&self) -> FieldResult<DeviceStatus> {
        let status = tokio::task::spawn_blocking(crate::nas::device_info::collect_device_status)
            .await
            .map_err(|e| format!("device status collection failed: {e}"))??;
        Ok(DeviceStatus {
            uptime_sec: Long(status.uptime_sec),
            // A NAS has no battery — the doc's availability matrix keeps
            // batteryLevel null and charging false on Linux boxes.
            battery_level: None,
            charging: false,
            temperatures: status
                .temperatures
                .into_iter()
                .map(|t| Temperature {
                    label: t.label,
                    celsius: t.celsius,
                })
                .collect(),
            cpu_usage: status.cpu_usage,
            memory_available: status.memory_available.map(Long),
            storage_available: Long(crate::nas::mounts::df_usage("/").1),
        })
    }

    // ----- Developer pages (plain-app contract) -----

    /// Paths of the two SQLite stores behind the database page
    /// (`chat.db` + `library.db`), comma-joined — plain-app/desktop serve
    /// one db file, NAS has two.
    async fn db_path(&self) -> FieldResult<String> {
        Ok(format!(
            "{}, {}",
            self.data_dir.join("chat.db").display(),
            self.data_dir.join("library.db").display()
        ))
    }

    /// Absolute path of the preferences file — plain-app DataStore /
    /// plain-desktop `prefs.json` equivalent.
    async fn data_store_path(&self) -> FieldResult<String> {
        Ok(crate::prefs::default_path(&self.data_dir)
            .to_string_lossy()
            .to_string())
    }

    /// Every preference entry, key sorted (plain-app DataStore
    /// preferences / plain-desktop `prefs.json` map).
    async fn data_store_entries(&self) -> FieldResult<Vec<KeyValuePair>> {
        Ok(self
            .prefs
            .entries_sorted()
            .into_iter()
            .map(|(key, value)| KeyValuePair { key, value })
            .collect())
    }

    /// Absolute path of the current log file.
    async fn app_log_path(&self) -> FieldResult<String> {
        Ok(crate::nas::log::default_log_file(&self.data_dir)
            .to_string_lossy()
            .to_string())
    }

    /// Log lines, newest first (plain-app `AppLogHelper` semantics);
    /// the DSL `text:` field of `query` is a case-insensitive substring
    /// over the line, applied before offset/limit.
    async fn app_logs(&self, offset: i32, limit: i32, query: String) -> FieldResult<Vec<String>> {
        let path = crate::nas::log::default_log_file(&self.data_dir);
        let needle = text_of(&query);
        Ok(crate::nas::log::read_lines_newest_first(
            &path,
            (!needle.trim().is_empty()).then_some(needle.as_str()),
            offset.max(0) as usize,
            limit.max(0) as usize,
        ))
    }

    /// Tables of the two SQLite stores (`chat.db` + `library.db`) by bare
    /// name, sorted — no store prefix.
    async fn db_tables(&self, ctx: &Context<'_>) -> FieldResult<Vec<String>> {
        let chat = ctx.data::<std::sync::Arc<crate::api::chat::ChatState>>()?;
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        Ok(crate::nas::devtools_sqlite::tables(
            &chat.service.db,
            library,
        ))
    }

    /// Entry count of one table.
    async fn db_table_row_count(&self, ctx: &Context<'_>, table: String) -> FieldResult<Long> {
        let chat = ctx.data::<std::sync::Arc<crate::api::chat::ChatState>>()?;
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        crate::nas::devtools_sqlite::table_row_count(&chat.service.db, library, &table)
            .map(Long)
            .map_err(|e| async_graphql::Error::new(e.to_string()))
    }

    /// One page of a table's rows (`SELECT *`), each a JSON string
    /// (numbers stay numbers, blobs render as hex).
    async fn db_table_rows(
        &self,
        ctx: &Context<'_>,
        table: String,
        offset: i32,
        limit: i32,
    ) -> FieldResult<Vec<String>> {
        let chat = ctx.data::<std::sync::Arc<crate::api::chat::ChatState>>()?;
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        crate::nas::devtools_sqlite::table_rows(
            &chat.service.db,
            library,
            &table,
            offset as i64,
            limit as i64,
        )
        .map_err(|e| async_graphql::Error::new(e.to_string()))
    }

    /// Row identity field of a table (its declared primary key column;
    /// composite keys use the first key column).
    async fn db_table_info(&self, ctx: &Context<'_>, table: String) -> FieldResult<DbTableInfo> {
        let chat = ctx.data::<std::sync::Arc<crate::api::chat::ChatState>>()?;
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        Ok(DbTableInfo {
            id_key: crate::nas::devtools_sqlite::table_id_key(&chat.service.db, library, &table)
                .map_err(|e| async_graphql::Error::new(e.to_string()))?,
        })
    }

    /// Column metadata of a table (`PRAGMA table_info`).
    async fn db_table_columns(
        &self,
        ctx: &Context<'_>,
        table: String,
    ) -> FieldResult<Vec<DbTableColumn>> {
        let chat = ctx.data::<std::sync::Arc<crate::api::chat::ChatState>>()?;
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        crate::nas::devtools_sqlite::table_columns(&chat.service.db, library, &table)
            .map(|cols| {
                cols.into_iter()
                    .map(|c| DbTableColumn {
                        name: c.name,
                        data_type: column_type_of(&c.data_type),
                        not_null: c.not_null,
                        default_value: c.default_value,
                        primary_key: c.primary_key,
                    })
                    .collect()
            })
            .map_err(|e| async_graphql::Error::new(e.to_string()))
    }

    /// List of mounted volumes + unmounted partitions (Go `ListMounts`).
    async fn mounts(&self, ctx: &Context<'_>) -> FieldResult<Vec<StorageMount>> {
        let prefs = ctx.data::<std::sync::Arc<crate::prefs::Prefs>>()?;
        Ok(crate::nas::mounts::list_mounts(prefs)
            .into_iter()
            .map(|m| StorageMount {
                id: m.id.into(),
                name: m.name,
                path: m.path.unwrap_or_default(),
                partition_num: m.partition_num,
                label: m.label,
                uuid: m.uuid,
                mount_point: m.mount_point.unwrap_or_default(),
                fs_type: m.fs_type.unwrap_or_default(),
                total_bytes: Long(m.total_bytes),
                used_bytes: Long(m.used_bytes.unwrap_or(0)),
                free_bytes: Long(m.free_bytes.unwrap_or(0)),
                alias: m.alias.unwrap_or_default(),
                remote: m.remote,
                // /proc/mounts exposes no bus kind — everything on a NAS is
                // served as internal storage (plain-app DriveType contract).
                drive_type: DriveType::INTERNAL_STORAGE,
                disk_id: m.disk_id.unwrap_or_default(),
            })
            .collect())
    }

    /// List of block devices.
    async fn disks(&self) -> FieldResult<Vec<StorageDisk>> {
        Ok(crate::nas::storage_disks::list_disks()
            .into_iter()
            .map(|d| StorageDisk {
                id: d.id.into(),
                name: d.name,
                path: d.path,
                size_bytes: Long(d.size_bytes),
                removable: d.removable,
                model: d.model,
            })
            .collect())
    }

    /// Active sessions.
    async fn sessions(&self) -> FieldResult<Vec<Session>> {
        let list = crate::media::kv::SessionStore::new(&self.db).list();
        Ok(list
            .into_iter()
            .map(|s| Session {
                client_id: s.client_id,
                client_name: s.client_name,
                last_active: Instant(s.last_active),
                created_at: Instant(s.created_at),
                updated_at: Instant(s.updated_at),
            })
            .collect())
    }

    /// Audit events, newest first. The DSL `text:` field of `query` is a
    /// case-insensitive substring over `type` + `message`, applied before
    /// offset/limit. (Named `auditEvents` — "event" alone collides with
    /// the WS push events and the scan/chat events.)
    async fn audit_events(
        &self,
        offset: i32,
        limit: i32,
        query: String,
    ) -> FieldResult<Vec<AuditEvent>> {
        let needle = text_of(&query);
        let list = crate::media::kv::EventLog::new(&self.db).list(
            offset.max(0) as usize,
            limit.max(0) as usize,
            (!needle.trim().is_empty()).then_some(needle.as_str()),
        )?;
        Ok(list
            .into_iter()
            .filter_map(|e| {
                // Rows with an unknown kind (older build) are skipped, not
                // migrated — stale derived data heals by attrition.
                let r#type = AuditEventType::from_kind(&e.r#type)?;
                Some(AuditEvent {
                    id: e.id.into(),
                    r#type,
                    message: e.message,
                    client_id: e.client_id,
                    created_at: Instant(e.created_at),
                })
            })
            .collect())
    }

    /// List all favorite folders.
    async fn favorite_folders(&self, ctx: &Context<'_>) -> FieldResult<Vec<FavoriteFolder>> {
        let library = ctx.data::<std::sync::Arc<crate::library::db::LibraryDb>>()?;
        let items = crate::library::favorite_folders::list(library);
        Ok(items
            .into_iter()
            .map(super::mutation::favorite_to_gql)
            .collect())
    }

    /// List uploaded chunk indices for a given fileID, as strings
    /// (plain-app `uploadedChunks(fileId: String!): [String!]!`).
    async fn uploaded_chunks(
        &self,
        _ctx: &Context<'_>,
        #[graphql(name = "fileId")] file_id: String,
    ) -> FieldResult<Vec<String>> {
        let paths = crate::nas::consts::AppPaths::detect();
        let list =
            crate::nas::chunked_upload::list_uploaded_chunks(&paths.data_dir, &file_id).await?;
        Ok(list.into_iter().map(|i| i.to_string()).collect())
    }

    /// App update check. Sync (ureq, 5s timeout), cached 10 min in-process.
    /// Wrapped in `spawn_blocking` so it doesn't tie up the async runtime.
    async fn app_update(&self, _ctx: &Context<'_>) -> FieldResult<AppUpdate> {
        let u = tokio::task::spawn_blocking(crate::nas::app_update::app_update)
            .await
            .map_err(|e| async_graphql::Error::new(format!("join: {e}")))?;
        Ok(AppUpdate {
            current_version: u.current_version,
            latest_version: u.latest_version,
            has_update: u.has_update,
            url: u.url,
        })
    }

    // ----- Samba -----

    /// Current LAN-share (samba) settings: enabled flag, provisioned username/password state, shares and the backing systemd unit's status.
    async fn samba_settings(&self, ctx: &Context<'_>) -> FieldResult<SambaSettings> {
        let prefs = ctx.data::<std::sync::Arc<crate::prefs::Prefs>>()?;
        let s = crate::nas::samba::get_samba_settings(prefs);
        // Live unit status (Go `sambaSettings` resolver); persisted
        // service_* fields are only fallbacks for non-systemd hosts.
        let service = crate::nas::samba::get_service_status();
        let (name, active, enabled) = if service.name.is_empty() {
            (s.service_name.clone(), s.service_active, s.service_enabled)
        } else {
            (service.name, service.active, service.enabled)
        };
        let shares: Vec<SambaShare> = s
            .shares
            .into_iter()
            .map(|sh| {
                let auth = match sh.auth {
                    crate::nas::samba::SambaShareAuth::Guest => SambaShareAuth::GUEST,
                    crate::nas::samba::SambaShareAuth::Password => SambaShareAuth::PASSWORD,
                };
                SambaShare {
                    name: sh.name,
                    share_path: sh.share_path,
                    auth,
                    read_only: sh.read_only,
                }
            })
            .collect();
        Ok(SambaSettings {
            enabled: s.enabled,
            username: s.username,
            has_password: s.has_password,
            shares,
            service_name: name,
            service_active: active,
            service_enabled: enabled,
        })
    }

    // ----- DLNA -----

    /// DLNA renderers discovered on the LAN via SSDP (refreshed on every `dlnaRenderers` fetch).
    async fn dlna_renderers(&self, ctx: &Context<'_>) -> FieldResult<Vec<DlnaRenderer>> {
        // Mirrors Go `dlnaRenderersModel`: kick off (or join) the
        // long-running discovery task with the caller's client_id so the
        // result streams back over WS, then return the current cache.
        let cid = ctx.data::<String>().ok().cloned().unwrap_or_default();
        if !cid.is_empty() {
            crate::nas::dlna::start_renderer_discovery(&cid);
        }
        let rs = crate::nas::dlna::cached_renderers();
        let out = rs
            .into_iter()
            .map(|r| DlnaRenderer {
                udn: r.udn,
                name: r.name,
                manufacturer: if r.manufacturer.is_empty() {
                    None
                } else {
                    Some(r.manufacturer)
                },
                model_name: if r.model_name.is_empty() {
                    None
                } else {
                    Some(r.model_name)
                },
                location: r.location,
            })
            .collect();
        Ok(out)
    }

    // ----- Media source dirs -----

    // ----- Path predicates (pathExists / pathKind) -----

    /// Detailed info for a single media file used by the lightbox UI.
    /// `path` is the canonical lookup key (round-7 contract: `id` addressing
    /// removed; file tags lazy-load via `tagRelations` on the client).
    async fn file_info(
        &self,
        ctx: &Context<'_>,
        path: String,
        #[graphql(name = "fileName")] _file_name: Option<String>,
        _include_dir_size: Option<bool>,
    ) -> FieldResult<FileInfo> {
        let _ = _include_dir_size;
        let p = std::path::Path::new(&path);
        let entry = crate::media::fsx::stat(p)
            .await
            .map_err(|e| async_graphql::Error::new(format!("stat: {e}")))?;

        // Populate `data` field based on file type (mirrors Go `buildFileInfo`).
        // Every branch reads the file (row hydration for indexed audio/video,
        // header probes otherwise), so the whole computation runs off the
        // async runtime.
        let db = ctx
            .data::<std::sync::Arc<crate::media::kv::Db>>()
            .ok()
            .cloned();
        let data = {
            let path = path.clone();
            super::run_blocking(move || build_file_info_data(db.as_deref(), &path)).await?
        };

        Ok(FileInfo {
            path: entry.path,
            updated_at: Instant(entry.updated_at),
            size: Long(entry.size),
            data,
        })
    }

    // ----- Image / Video / Audio list (media index) -----
    //
    // Backed by the tantivy media search index, which the scan pipeline
    // (`media_scan::scan_tree`), the watcher and the delete paths keep in
    // sync with the KV media rows. `query` uses the app search DSL
    // (`trash:true`, `excluded_dir:x`, `size:>10MB`, bare text, …).

    // ----- Docs (plain-app DocGraphQL parity) -----
    //
    // Doc rows are ordinary media-index rows whose `infer_type` classified
    // them as "doc" (text/* + office extensions via the shared MIME table).
}

/// Stored track duration is seconds; the wire contract is `durationMs`
/// (plain-app), so convert here.
pub(crate) fn playlist_audio_to_gql(a: crate::library::audio_queue::AudioTrack) -> AudioItem {
    AudioItem {
        title: a.title,
        artist: a.artist,
        path: a.path,
        duration_ms: Long(a.duration_secs as i64 * 1000),
    }
}

/// Locate an executable on PATH (capability detection: doc preview /
/// disk manager look for soffice / lsblk).
fn which(name: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    for p in std::env::split_paths(&path) {
        let candidate = p.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Parse the stored play-mode name into the GraphQL enum (REPEAT default).
fn media_play_mode_of(library: &crate::library::db::LibraryDb) -> MediaPlayMode {
    match crate::library::audio_queue::get_audio_mode(library).as_str() {
        "REPEAT_ONE" => MediaPlayMode::REPEAT_ONE,
        "SHUFFLE" => MediaPlayMode::SHUFFLE,
        _ => MediaPlayMode::REPEAT,
    }
}

/// Media-index count for one media type under the app search DSL.

/// GraphQL `FileSortBy` → media index sort order. TAKEN_AT_DESC only
/// applies to capture-date grouping (API_SPEC §3); the media index sorts
/// those by mtime-descending instead.

/// The media bucket of a file is its containing directory.

/// Map a media-index row (kind `doc`) to the GraphQL `Doc` type.
/// Mirrors plain-app `DDoc.toDocModel` — title is the display name,
/// extension the lowercased filename extension.

/// Unix seconds → Instant (UTC), epoch when unknown.

/// Tags related to a media key (the GraphQL media id), filtered to the data
/// type — mirrors plain-app `TagsLoader.load(id, type)`, which reads
/// relations by media id with the DataType filter. Best effort: a broken
/// tag store yields no tags rather than failing the whole list.

/// Test seam: serializes tests that WRITE the process-global media search
/// index (`search_index::global()` — clear/add/remove/commit). The index is
/// one OnceLock per process, so concurrent writer tests interleave tantivy
/// commits and observably drop each other's rows; the lock turns that
/// shared-resource race into a queue.
#[cfg(test)]
pub(crate) static GLOBAL_INDEX_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Public seam so `types.rs` ComplexObject resolvers can reach the loader.

/// Numeric tag kind for a `DataType` (0=DEFAULT, 1=AUDIO, 2=VIDEO, 3=IMAGE).

/// Convert a `crate::media::trash::TrashItem` into the GraphQL `File` type.
/// Mirrors Go `trashItemToModel` in `internal/graph/helpers/files_query_helper.go`:
///   * `path` = `<disk>/.nas-trash/<trash_rel_path>` (the physical trashed path)
///   * `is_dir` = (kind == "dir")
///   * `created_at` = `updated_at` = `deleted_at` (unix seconds → RFC3339)
///   * `size` = it.size.unwrap_or(0)
///   * `children` = entry_count - 1 when dir and entry_count > 1, else 0

/// The `text:` field of the shared search DSL (`crate::media::search::parse`),
/// used by the paginated list ops that filter on plain-app's server-side
/// `text:` extraction. Empty string = no filtering.
fn text_of(query: &str) -> String {
    crate::media::search::parse(query)
        .into_iter()
        .find(|f| f.name == "text")
        .map(|f| f.value)
        .unwrap_or_default()
}

/// Total metadata probe: `None` for blank/`.` paths, missing files and
/// stat errors — `pathExists`/`pathKind` never raise on those.

/// i64 scanner counter → GraphQL Int. File counts can never reach 2^31 in
/// practice; the saturating cast keeps the mapping total.

/// Resolve the directory a `files` query lists: `parent` (DSL) overrides
/// `root` (plain-app argument), which overrides `root_path` (legacy Go DSL
/// field); `relative_path` (legacy Go DSL) joins underneath. Empty input
/// lists `/`.

/// `FileInfo.data` for one path. Indexed audio/video hydrate the media
/// row's cached duration (probe once per file version, persist); everything
/// else falls back to the transient header probe. The extension gate keeps
/// images and non-media files from paying a pointless KV row read.
/// Blocking (file I/O) — call via `super::run_blocking`.
fn build_file_info_data(db: Option<&crate::media::kv::Db>, path: &str) -> Option<MediaFileInfo> {
    let av = matches!(crate::media::scan::infer_type(path), "audio" | "video");
    if av
        && let Some(db) = db
        && let Some(mut mf) = crate::media::scan::get_by_path(db, path).ok().flatten()
        && matches!(mf.r#type.as_str(), "audio" | "video")
    {
        crate::media::scan::hydrate_metadata(db, &mut mf);
        // durationMs is non-null on the wire (plain-app contract); an
        // unprobed duration serves 0.
        let duration_ms = Long((mf.duration_sec * 1000) as i64);
        return if mf.r#type == "video" {
            Some(MediaFileInfo::Video(VideoFileInfo {
                width: 0,
                height: 0,
                duration_ms,
                location: None,
            }))
        } else {
            Some(MediaFileInfo::Audio(AudioFileInfo {
                duration_ms,
                location: None,
            }))
        };
    }
    probe_file_info_data(path)
}

/// Probe file metadata for the `FileInfo.data` union field.
/// Mirrors Go `buildFileInfo` logic for populating image/video/audio metadata.
/// Blocking (file I/O) — call via `super::run_blocking`.
fn probe_file_info_data(path: &str) -> Option<MediaFileInfo> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // Image files: read dimensions using the `image` crate.
    if matches!(
        ext.as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tiff" | "tif"
    ) {
        return probe_image_dimensions(path);
    }

    // Video files: probe duration using `lofty`.
    if matches!(
        ext.as_str(),
        "mp4" | "m4v" | "mov" | "mkv" | "avi" | "webm" | "flv" | "wmv"
    ) {
        let duration_secs = crate::media::metadata::probe_duration_secs(path).unwrap_or(0);
        return Some(MediaFileInfo::Video(VideoFileInfo {
            width: 0,
            height: 0,
            duration_ms: Long((duration_secs * 1000) as i64),
            location: None,
        }));
    }

    // Audio files: probe duration using `lofty`.
    if matches!(
        ext.as_str(),
        "mp3" | "flac" | "ogg" | "opus" | "m4a" | "wav" | "aac" | "wma"
    ) {
        let duration_secs = crate::media::metadata::probe_duration_secs(path).unwrap_or(0);
        return Some(MediaFileInfo::Audio(AudioFileInfo {
            duration_ms: Long((duration_secs * 1000) as i64),
            location: None,
        }));
    }

    None
}

/// Read image dimensions without decoding the full image (mirrors Go `image.DecodeConfig`).
fn probe_image_dimensions(path: &str) -> Option<MediaFileInfo> {
    use image::ImageReader;
    use std::fs::File;
    use std::io::BufReader;

    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    let img_reader = ImageReader::new(reader).with_guessed_format().ok()?;
    let (width, height) = img_reader.into_dimensions().ok()?;

    Some(MediaFileInfo::Image(ImageFileInfo {
        width: width as i32,
        height: height as i32,
        location: None,
    }))
}

#[cfg(test)]
#[path = "../../../../tests/unit/api/schema/nas/query.rs"]
mod tests;
