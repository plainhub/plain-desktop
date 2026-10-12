//! Public `/graphql` audio roots: the library list, the playback queue, user
//! playlists, play history and the player's own state.
//!
//! The queue, the playlists and the history are Rust SQLite tables reached
//! through [`crate::services::audio::host::Audio`], which is the same service the app's own
//! `audioHost*` roots use — so the web client and the app see one queue, not
//! two. What stays on the platform is genuinely platform work: enumerating
//! MediaStore, reading a file's metadata and lyrics, and driving the player.

use crate::graphql::resolvers::tags::domain_types::Tag;
use crate::graphql::types::Instant;
use crate::graphql::types::Long;
use crate::host::bridge::Host;
use crate::http::auth::public_gate;
use crate::services::audio::host::Audio as AudioService;
use async_graphql::{Context, Object, SimpleObject};
use plain_rs::db::PlayHistory;
use plain_rs::db::Playlist;
use plain_rs::media::audio::queue as domain;
use plain_rs::media::audio::queue::AudioTrack;
use serde_json::{Value, json};
use std::sync::Arc;

const STORAGE: &str = "WRITE_EXTERNAL_STORAGE";

pub use plain_rs::media::audio::play_mode::MediaPlayMode;

#[derive(SimpleObject, Clone, Debug)]
pub struct AudioItem {
    pub title: String,
    pub artist: String,
    /// Album id from the platform catalog, null when the track carries no
    /// album metadata.
    #[graphql(name = "albumId")]
    pub album_id: Option<async_graphql::ID>,
    pub path: String,
    #[graphql(name = "durationMs")]
    pub duration_ms: Long,
}

impl From<AudioTrack> for AudioItem {
    fn from(track: AudioTrack) -> Self {
        item(track)
    }
}

#[derive(SimpleObject, Clone, Debug)]
#[graphql(complex)]
pub struct Audio {
    pub id: async_graphql::ID,
    pub title: String,
    pub artist: String,
    pub path: String,
    #[graphql(name = "durationMs")]
    pub duration_ms: Long,
    pub size: Long,
    #[graphql(name = "bucketId")]
    pub bucket_id: async_graphql::ID,
    /// FileId of the album-art image, usable in file display URLs; empty
    /// when the track has no album art.
    #[graphql(name = "albumFileId")]
    pub album_file_id: String,
    #[graphql(name = "createdAt")]
    pub created_at: Instant,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
    #[graphql(name = "isFavorite")]
    pub is_favorite: bool,
}

#[async_graphql::ComplexObject]
impl Audio {
    async fn tags(&self, ctx: &async_graphql::Context<'_>) -> async_graphql::Result<Vec<Tag>> {
        crate::graphql::resolvers::tags::loader::load(
            ctx,
            plain_rs::enums::DataType::Audio,
            self.id.as_str(),
        )
        .await
    }
}

#[derive(SimpleObject, Clone, Debug)]
pub struct AudioPlayHistory {
    pub path: String,
    pub title: String,
    pub artist: String,
    #[graphql(name = "durationMs")]
    pub duration_ms: Long,
    #[graphql(name = "playCount")]
    pub play_count: i32,
    #[graphql(name = "playedAt")]
    pub played_at: Instant,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct AudioPlaylist {
    pub id: async_graphql::ID,
    pub name: String,
    #[graphql(name = "itemCount")]
    pub item_count: i32,
    #[graphql(name = "createdAt")]
    pub created_at: Instant,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct AudioPlayback {
    /// Null while the player is idle — the queue stores an empty string as
    /// "no current track", and the contract says `null`, not `""`.
    #[graphql(name = "currentPath")]
    pub current_path: Option<String>,
    pub mode: MediaPlayMode,
    #[graphql(name = "isPlaying")]
    pub is_playing: bool,
    #[graphql(name = "positionMs")]
    pub position_ms: Long,
}

#[derive(Default)]
pub struct AudioQuery;

#[Object]
impl AudioQuery {
    async fn audio_count(&self, ctx: &Context<'_>, query: String) -> async_graphql::Result<i32> {
        if !public_gate::require_granted(ctx, &[STORAGE]).await? {
            return Ok(0);
        }
        let count = host_call(
            ctx,
            "systemMediaCount",
            json!({ "dataType": "AUDIO", "query": query }),
        )
        .await?;
        Ok(count.as_i64().unwrap_or_default() as i32)
    }

    async fn audios(
        &self,
        ctx: &Context<'_>,
        query: String,
        offset: i32,
        limit: i32,
        sort_by: crate::graphql::resolvers::files::types::FileSortBy,
    ) -> async_graphql::Result<Vec<Audio>> {
        public_gate::require(ctx, prefs(ctx), &[STORAGE])?;
        let items = crate::graphql::types::facts::rows(
            &host_call(
                ctx,
                "systemMediaRows",
                json!({
                    "dataType": "AUDIO", "query": query,
                    "offset": offset, "limit": limit, "sortBy": sort_by.as_str(),
                }),
            )
            .await?,
            |item| item.clone(),
        );
        Ok(items
            .iter()
            .map(|item| {
                let id = crate::graphql::types::facts::id(item, "id");
                Audio {
                    id: id.clone(),
                    title: crate::graphql::types::facts::text(item, "title"),
                    artist: crate::graphql::types::facts::text(item, "artist"),
                    path: crate::graphql::types::facts::text(item, "path"),
                    duration_ms: Long(crate::graphql::types::facts::integer(item, "durationMs")),
                    size: Long(crate::graphql::types::facts::integer(item, "size")),
                    bucket_id: crate::graphql::types::facts::id(item, "bucketId"),
                    album_file_id: crate::graphql::types::facts::text(item, "albumFileId"),
                    created_at: crate::graphql::types::facts::instant(item, "createdAt"),
                    updated_at: crate::graphql::types::facts::instant(item, "updatedAt"),
                    is_favorite: crate::graphql::types::facts::flag(item, "isFavorite"),
                }
            })
            .collect())
    }

    /// Null rather than an empty string when the track carries no lyrics:
    /// "no lyrics file" and "a lyrics file that is empty" are the same
    /// answer to a client, and only one of them survives the wire.
    async fn audio_lyrics(
        &self,
        ctx: &Context<'_>,
        path: String,
    ) -> async_graphql::Result<Option<String>> {
        public_gate::require(ctx, prefs(ctx), &[STORAGE])?;
        let lyrics = host_call(ctx, "systemAudioLyrics", json!({ "path": path })).await?;
        Ok(lyrics
            .as_str()
            .filter(|value| !value.is_empty())
            .map(str::to_string))
    }

    /// The active queue, paged. Filtering is on the item title and artist,
    /// which is the same text a client would show.
    async fn audio_queue_items(
        &self,
        ctx: &Context<'_>,
        query: String,
        offset: i32,
        limit: i32,
    ) -> async_graphql::Result<Vec<AudioItem>> {
        public_gate::require(ctx, prefs(ctx), &[STORAGE])?;
        Ok(audio(ctx)?
            .run(move |db, lib| domain::queue_page(db, lib, offset as i64, limit as i64, &query))
            .await?
            .into_iter()
            .map(item)
            .collect())
    }

    async fn audio_queue_item_count(&self, ctx: &Context<'_>) -> async_graphql::Result<i32> {
        public_gate::require(ctx, prefs(ctx), &[STORAGE])?;
        Ok(audio(ctx)?
            .run(|db, lib| domain::queue_total(db, lib))
            .await?
            .try_into()?)
    }

    async fn audio_playlists(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<AudioPlaylist>> {
        Ok(audio(ctx)?
            .run(|db, _| domain::playlists(db))
            .await?
            .into_iter()
            .map(|(row, count)| playlist(row, count))
            .collect::<async_graphql::Result<Vec<_>>>()?)
    }

    async fn audio_playlist(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<Option<AudioPlaylist>> {
        let key = id.as_str().to_string();
        let fetched = audio(ctx)?
            .run(move |db, _| {
                domain::playlist_by_id(db, &key)?
                    .map(|row| {
                        Ok::<(Playlist, usize), plain_rs::error::DomainError>((
                            row,
                            domain::playlist_item_count(db, &key)?,
                        ))
                    })
                    .transpose()
            })
            .await?;
        fetched.map(|(row, count)| playlist(row, count)).transpose()
    }

    async fn audio_playlist_items(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        query: String,
        offset: i32,
        limit: i32,
    ) -> async_graphql::Result<Vec<AudioItem>> {
        Ok(audio(ctx)?
            .run(move |db, _| {
                domain::playlist_item_records_page(
                    db,
                    id.as_str(),
                    offset as i64,
                    limit as i64,
                    &query,
                )
            })
            .await?
            .into_iter()
            .map(playlist_item)
            .collect())
    }

    async fn audio_playlist_item_count(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<i32> {
        Ok(audio(ctx)?
            .run(move |db, _| domain::playlist_item_count(db, id.as_str()))
            .await?
            .try_into()?)
    }

    async fn audio_play_history(
        &self,
        ctx: &Context<'_>,
        query: String,
        offset: i32,
        limit: i32,
    ) -> async_graphql::Result<Vec<AudioPlayHistory>> {
        Ok(audio(ctx)?
            .run(move |db, _| domain::history_page(db, offset as i64, limit as i64, &query))
            .await?
            .into_iter()
            .map(history)
            .collect::<async_graphql::Result<Vec<_>>>()?)
    }

    /// Half of this is a store read (the mode is a user preference, the
    /// current path is the queue's) and half is the player itself, which is
    /// platform state Rust cannot see.
    async fn audio_playback(&self, ctx: &Context<'_>) -> async_graphql::Result<AudioPlayback> {
        let current = audio(ctx)?
            .run(|db, _| domain::get_audio_current(db))
            .await?;
        let mode = domain::play_mode(prefs(ctx))?;
        let player = host_call(ctx, "systemAudioPlaybackState", json!({})).await?;
        Ok(AudioPlayback {
            current_path: Some(current).filter(|path| !path.is_empty()),
            mode,
            is_playing: crate::graphql::types::facts::flag(&player, "isPlaying"),
            position_ms: Long(crate::graphql::types::facts::integer(&player, "positionMs")),
        })
    }
}

#[derive(Default)]
pub struct AudioMutation;

#[Object]
impl AudioMutation {
    /// Marks the track current without touching the queue, which is what a
    /// "play this one row" tap means. The metadata comes from the platform
    /// because the queue stores whatever the file says.
    async fn play_audio(
        &self,
        ctx: &Context<'_>,
        path: String,
    ) -> async_graphql::Result<AudioItem> {
        let facts = host_call(ctx, "systemAudioPlaylistTracks", json!({ "paths": [path] })).await?;
        let track = crate::graphql::types::facts::rows(&facts, |item| item.clone())
            .into_iter()
            .next()
            .ok_or_else(|| async_graphql::Error::new("audio metadata unavailable"))?;
        let item = AudioItem {
            title: crate::graphql::types::facts::text(&track, "title"),
            artist: crate::graphql::types::facts::text(&track, "artist"),
            album_id: album_id_option(crate::graphql::types::facts::text(&track, "albumId")),
            path: crate::graphql::types::facts::text(&track, "path"),
            duration_ms: Long(crate::graphql::types::facts::integer(&track, "durationMs")),
        };
        audio(ctx)?
            .run({
                let path = item.path.clone();
                move |db, _| domain::save_audio_current(db, &path)
            })
            .await?;
        Ok(item)
    }

    async fn update_audio_play_mode(
        &self,
        ctx: &Context<'_>,
        mode: MediaPlayMode,
    ) -> async_graphql::Result<bool> {
        crate::graphql::resolvers::settings::state::apply_user_patch(
            crate::graphql::resolvers::settings::state::UserPatchEnv::from_ctx(ctx)?,
            plain_rs::prefs::user::UserSettingsPatch {
                audio_play_mode: Some(mode.ordinal()),
                ..Default::default()
            },
        )
        .await?;
        Ok(true)
    }

    /// Clearing the queue is two actions: the rows go, and the player is
    /// told to stop. Leaving the second to the app would leave a player
    /// running over an empty queue.
    async fn clear_audio_queue(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        audio(ctx)?.run(|db, _| domain::clear_queue(db)).await?;
        let prefs = prefs(ctx).clone();
        audio(ctx)?
            .run(move |db, engine| {
                plain_rs::media::audio::commands::command(
                    db,
                    &prefs,
                    engine,
                    plain_rs::media::audio::commands::Action::Clear,
                    0,
                    1.0,
                )
            })
            .await?;
        notify_playback(ctx)?;
        Ok(true)
    }

    async fn remove_audio_from_queue(
        &self,
        ctx: &Context<'_>,
        path: String,
    ) -> async_graphql::Result<bool> {
        audio(ctx)?
            .run(move |db, _| domain::remove_queued(db, &path))
            .await?;
        Ok(true)
    }

    /// The cap is the same 1000 plain-app uses: this is "queue these", not a
    /// bulk import, and a client that means everything should page instead.
    async fn add_audios_to_queue(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> async_graphql::Result<bool> {
        let sort_by = library_sort(ctx)?;
        let facts = host_call(
            ctx,
            "systemAudioSearchTracks",
            json!({ "query": query, "limit": QUEUE_SEARCH_LIMIT, "offset": 0, "sortBy": sort_by.as_str() }),
        )
        .await?;
        let tracks = crate::graphql::types::facts::rows(&facts, |item| track(item));
        audio(ctx)?
            .run(move |db, _| domain::enqueue(db, &tracks, false))
            .await?;
        Ok(true)
    }

    async fn reorder_audio_queue(
        &self,
        ctx: &Context<'_>,
        paths: Vec<String>,
    ) -> async_graphql::Result<bool> {
        audio(ctx)?
            .run(move |db, _| domain::reorder_queued(db, &paths))
            .await?;
        Ok(true)
    }

    async fn create_audio_playlist(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> async_graphql::Result<AudioPlaylist> {
        let row = audio(ctx)?
            .run(move |db, _| domain::create_playlist(db, &name))
            .await?;
        playlist(row, 0)
    }

    async fn update_audio_playlist(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        name: String,
    ) -> async_graphql::Result<AudioPlaylist> {
        let key = id.to_string();
        audio(ctx)?
            .run({
                let key = key.clone();
                move |db, _| domain::rename_playlist(db, &key, &name)
            })
            .await?;
        let row = audio(ctx)?
            .run({
                let key = key.clone();
                move |db, _| domain::playlist_by_id(db, &key)
            })
            .await?
            .ok_or_else(|| {
                async_graphql::Error::new(format!("Playlist {key} not found after update"))
            })?;
        let count = audio(ctx)?
            .run(move |db, _| domain::playlist_item_count(db, &key))
            .await?;
        playlist(row, count)
    }

    async fn delete_audio_playlist(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        audio(ctx)?
            .run(move |db, _| domain::delete_playlist(db, id.as_str()))
            .await?;
        Ok(true)
    }

    async fn add_audio_playlist_items(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        paths: Vec<String>,
    ) -> async_graphql::Result<bool> {
        let facts = host_call(ctx, "systemAudioPlaylistTracks", json!({ "paths": paths })).await?;
        let tracks = crate::graphql::types::facts::rows(&facts, |item| track(item));
        audio(ctx)?
            .run(move |db, _| domain::add_playlist_items(db, id.as_str(), &tracks))
            .await?;
        Ok(true)
    }

    async fn remove_audio_playlist_item(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        path: String,
    ) -> async_graphql::Result<bool> {
        audio(ctx)?
            .run(move |db, _| domain::remove_playlist_item(db, id.as_str(), &path))
            .await?;
        Ok(true)
    }

    /// Null when there is nothing to play — an empty playlist, or a start
    /// path that is no longer in it — rather than an error, because "it
    /// played nothing" is the honest answer and the caller can show it.
    async fn play_audio_playlist(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        path: Option<String>,
        shuffle: bool,
    ) -> async_graphql::Result<Option<AudioItem>> {
        let start = audio(ctx)?
            .run(move |db, _| domain::select_playlist_source(db, id.as_str(), path.as_deref()))
            .await?;
        self.start_playback(ctx, start, shuffle).await
    }

    async fn play_all_audios(
        &self,
        ctx: &Context<'_>,
        shuffle: bool,
    ) -> async_graphql::Result<Option<AudioItem>> {
        let sort_by = library_sort(ctx)?;
        let start = audio(ctx)?
            .run({
                let sort_by = sort_by.as_str().to_string();
                move |db, lib| domain::select_library_source(db, lib, None, shuffle, &sort_by)
            })
            .await?;
        self.start_playback(ctx, start, shuffle).await
    }
}

/// Shared tail of the two "start playing" mutations: the queue side is
/// already done, so all that is left is handing the track to the player.
impl AudioMutation {
    async fn start_playback(
        &self,
        ctx: &Context<'_>,
        start: Option<AudioTrack>,
        shuffle: bool,
    ) -> async_graphql::Result<Option<AudioItem>> {
        let Some(start) = start else {
            return Ok(None);
        };
        let track = if shuffle {
            audio(ctx)?
                .run(|db, lib| domain::select_next(db, lib, true, true))
                .await?
        } else {
            Some(start)
        };
        let Some(track) = track else {
            return Ok(None);
        };
        ctx.data::<Arc<Host>>()?
            .call_wait("systemAudioPlaybackPermission", json!({}))
            .await
            .map_err(async_graphql::Error::new)?;
        let item = item(track.clone());
        let prefs = prefs(ctx).clone();
        audio(ctx)?
            .run(move |db, engine| {
                plain_rs::media::audio::commands::play_track(db, &prefs, engine, track, false)
            })
            .await?;
        notify_playback(ctx)?;
        Ok(Some(item))
    }
}

fn notify_playback(ctx: &Context<'_>) -> async_graphql::Result<()> {
    let _ = ctx
        .data::<tokio::sync::broadcast::Sender<plain_rs::events::WsEvent>>()?
        .send(plain_rs::events::WsEvent::broadcast(
            "CONTENT_CHANGED",
            "{}".into(),
        ));
    Ok(())
}

const QUEUE_SEARCH_LIMIT: i64 = 1000;

fn item(track: AudioTrack) -> AudioItem {
    AudioItem {
        title: track.title,
        artist: track.artist,
        album_id: album_id_option(track.album_id),
        path: track.path,
        duration_ms: Long(track.duration_ms),
    }
}

fn album_id_option(value: String) -> Option<async_graphql::ID> {
    if value.is_empty() {
        None
    } else {
        Some(value.into())
    }
}

fn playlist_item(row: plain_rs::db::PlaylistItem) -> AudioItem {
    AudioItem {
        title: row.title,
        artist: row.artist,
        album_id: album_id_option(row.album_id),
        path: row.audio_path,
        duration_ms: Long(row.duration_ms),
    }
}

fn track(value: &Value) -> AudioTrack {
    AudioTrack {
        path: crate::graphql::types::facts::text(value, "path"),
        title: crate::graphql::types::facts::text(value, "title"),
        artist: crate::graphql::types::facts::text(value, "artist"),
        album_id: String::new(),
        duration_ms: crate::graphql::types::facts::integer(value, "durationMs"),
    }
}

fn playlist(row: Playlist, count: usize) -> async_graphql::Result<AudioPlaylist> {
    Ok(AudioPlaylist {
        id: row.id.into(),
        name: row.name,
        item_count: count.try_into()?,
        created_at: crate::graphql::resolvers::common::content_common::instant(&row.created_at)?,
        updated_at: crate::graphql::resolvers::common::content_common::instant(&row.updated_at)?,
    })
}

fn history(row: PlayHistory) -> async_graphql::Result<AudioPlayHistory> {
    Ok(AudioPlayHistory {
        path: row.path,
        title: row.title,
        artist: row.artist,
        duration_ms: Long(row.duration_ms),
        // The store counts in 64 bits; the contract says Int, and a play
        // count past two billion is not a number a client can act on.
        play_count: row.play_count.clamp(0, i64::from(i32::MAX)) as i32,
        played_at: crate::graphql::resolvers::common::content_common::instant(&row.played_at)?,
    })
}

fn audio<'a>(ctx: &'a Context<'_>) -> async_graphql::Result<&'a Arc<AudioService>> {
    ctx.data::<Arc<AudioService>>()
}

fn library_sort(
    ctx: &Context<'_>,
) -> async_graphql::Result<crate::graphql::resolvers::files::types::FileSortBy> {
    crate::graphql::resolvers::files::types::FileSortBy::from_ordinal(
        plain_rs::prefs::user::snapshot(prefs(ctx))?.audio_sort_by,
    )
}

fn prefs<'a>(ctx: &'a Context<'_>) -> &'a Arc<plain_rs::prefs::Prefs> {
    ctx.data_unchecked::<Arc<plain_rs::prefs::Prefs>>()
}

async fn host_call(ctx: &Context<'_>, method: &str, params: Value) -> async_graphql::Result<Value> {
    ctx.data_unchecked::<Arc<Host>>()
        .call(method, params)
        .await
        .map_err(async_graphql::Error::new)
}

#[cfg(test)]
#[path = "../../../../tests/unit/graphql/resolvers/audio/public.rs"]
mod tests;
