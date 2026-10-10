//! Public `/graphql` audio roots: the library list, the playback queue, user
//! playlists, play history and the player's own state.
//!
//! The queue, the playlists and the history are Rust SQLite tables reached
//! through [`super::audio::Audio`], which is the same service the app's own
//! `audioHost*` roots use — so the web client and the app see one queue, not
//! two. What stays on the platform is genuinely platform work: enumerating
//! MediaStore, reading a file's metadata and lyrics, and driving the player.

use super::host::Host;
use super::public_contact_types::Tag;
use super::public_gate;
use crate::content_api::audio::Audio as AudioService;
use crate::content_types::{Instant, Long};
use crate::db::{PlayHistory, Playlist};
use crate::library::audio_queue::{self as domain, AudioTrack};
use async_graphql::{Context, Enum, Object, SimpleObject};
use serde_json::{Value, json};
use std::sync::Arc;

const STORAGE: &str = "WRITE_EXTERNAL_STORAGE";

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum MediaPlayMode {
    #[default]
    Repeat,
    RepeatOne,
    Shuffle,
}

impl MediaPlayMode {
    fn as_str(self) -> &'static str {
        match self {
            MediaPlayMode::Repeat => "REPEAT",
            MediaPlayMode::RepeatOne => "REPEAT_ONE",
            MediaPlayMode::Shuffle => "SHUFFLE",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "REPEAT_ONE" => MediaPlayMode::RepeatOne,
            "SHUFFLE" => MediaPlayMode::Shuffle,
            _ => MediaPlayMode::Repeat,
        }
    }
}

#[derive(SimpleObject, Clone, Debug)]
pub struct AudioItem {
    pub title: String,
    pub artist: String,
    pub path: String,
    #[graphql(name = "durationMs")]
    pub duration_ms: Long,
}

#[derive(SimpleObject, Clone, Debug)]
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
    pub tags: Vec<Tag>,
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
        offset: i32,
        limit: i32,
        query: String,
        sort_by: crate::content_types::FileSortBy,
    ) -> async_graphql::Result<Vec<Audio>> {
        public_gate::require(prefs(ctx), &[STORAGE])?;
        let items = super::public_facts::rows(
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
        let tags =
            super::public_media::tags_for(ctx, "AUDIO", &Value::Array(items.clone())).await?;
        Ok(items
            .iter()
            .map(|item| {
                let id = super::public_facts::id(item, "id");
                Audio {
                    id: id.clone(),
                    title: super::public_facts::text(item, "title"),
                    artist: super::public_facts::text(item, "artist"),
                    path: super::public_facts::text(item, "path"),
                    duration_ms: Long(super::public_facts::integer(item, "durationMs")),
                    size: Long(super::public_facts::integer(item, "size")),
                    bucket_id: super::public_facts::id(item, "bucketId"),
                    album_file_id: super::public_facts::text(item, "albumFileId"),
                    created_at: super::public_facts::instant(item, "createdAt"),
                    updated_at: super::public_facts::instant(item, "updatedAt"),
                    is_favorite: super::public_facts::flag(item, "isFavorite"),
                    tags: tags.get(&id.to_string()).cloned().unwrap_or_default(),
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
        public_gate::require(prefs(ctx), &[STORAGE])?;
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
        offset: i32,
        limit: i32,
        query: String,
    ) -> async_graphql::Result<Vec<AudioItem>> {
        public_gate::require(prefs(ctx), &[STORAGE])?;
        Ok(audio(ctx)?
            .run(move |db, lib| domain::queue_page(db, lib, offset as i64, limit as i64, &query))
            .await?
            .into_iter()
            .map(item)
            .collect())
    }

    async fn audio_queue_item_count(&self, ctx: &Context<'_>) -> async_graphql::Result<i32> {
        public_gate::require(prefs(ctx), &[STORAGE])?;
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

    async fn audio_playlist_items(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        offset: i32,
        limit: i32,
        query: String,
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
            .map(|row| {
                Ok(AudioItem {
                    title: row.title,
                    artist: row.artist,
                    path: row.audio_path,
                    duration_ms: Long(row.duration_ms),
                })
            })
            .collect::<async_graphql::Result<Vec<_>>>()?)
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
        offset: i32,
        limit: i32,
        query: String,
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
        let mode = MediaPlayMode::parse(
            host_call(ctx, "systemAudioPlayMode", json!({}))
                .await?
                .as_str()
                .unwrap_or_default(),
        );
        let player = host_call(ctx, "systemAudioPlaybackState", json!({})).await?;
        Ok(AudioPlayback {
            current_path: Some(current).filter(|path| !path.is_empty()),
            mode,
            is_playing: super::public_facts::flag(&player, "isPlaying"),
            position_ms: Long(super::public_facts::integer(&player, "positionMs")),
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
        let track = super::public_facts::rows(&facts, |item| item.clone())
            .into_iter()
            .next()
            .ok_or_else(|| async_graphql::Error::new("audio metadata unavailable"))?;
        let item = AudioItem {
            title: super::public_facts::text(&track, "title"),
            artist: super::public_facts::text(&track, "artist"),
            path: super::public_facts::text(&track, "path"),
            duration_ms: Long(super::public_facts::integer(&track, "durationMs")),
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
        host_call(ctx, "systemAudioPlayMode", json!({ "mode": mode.as_str() })).await?;
        Ok(true)
    }

    /// Clearing the queue is two actions: the rows go, and the player is
    /// told to stop. Leaving the second to the app would leave a player
    /// running over an empty queue.
    async fn clear_audio_queue(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        audio(ctx)?.run(|db, _| domain::clear_queue(db)).await?;
        host_call(ctx, "systemAudioClear", json!({})).await?;
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
        let sort_by = host_call(ctx, "systemAudioLibrarySort", json!({})).await?;
        let facts = host_call(
            ctx,
            "systemAudioSearchTracks",
            json!({ "query": query, "limit": QUEUE_SEARCH_LIMIT, "offset": 0, "sortBy": sort_by.as_str().unwrap_or_default() }),
        )
        .await?;
        let tracks = super::public_facts::rows(&facts, |item| track(item));
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
        let tracks = super::public_facts::rows(&facts, |item| track(item));
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
        let sort_by = host_call(ctx, "systemAudioLibrarySort", json!({})).await?;
        let start = audio(ctx)?
            .run({
                let sort_by = sort_by.as_str().unwrap_or_default().to_string();
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
        let item = item(track);
        host_call(
            ctx,
            "systemAudioPlay",
            json!({ "track": { "title": item.title, "artist": item.artist, "path": item.path, "durationMs": item.duration_ms.0 } }),
        )
        .await?;
        Ok(Some(item))
    }
}

const QUEUE_SEARCH_LIMIT: i64 = 1000;

fn item(track: AudioTrack) -> AudioItem {
    AudioItem {
        title: track.title,
        artist: track.artist,
        path: track.path,
        duration_ms: Long(track.duration_ms),
    }
}

fn track(value: &Value) -> AudioTrack {
    AudioTrack {
        path: super::public_facts::text(value, "path"),
        title: super::public_facts::text(value, "title"),
        artist: super::public_facts::text(value, "artist"),
        album_id: String::new(),
        duration_ms: super::public_facts::integer(value, "durationMs"),
    }
}

fn playlist(row: Playlist, count: usize) -> async_graphql::Result<AudioPlaylist> {
    Ok(AudioPlaylist {
        id: row.id.into(),
        name: row.name,
        item_count: count.try_into()?,
        created_at: super::schema::content_common::instant(&row.created_at)?,
        updated_at: super::schema::content_common::instant(&row.updated_at)?,
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
        played_at: super::schema::content_common::instant(&row.played_at)?,
    })
}

fn audio<'a>(ctx: &'a Context<'_>) -> async_graphql::Result<&'a Arc<AudioService>> {
    ctx.data::<Arc<AudioService>>()
}

fn prefs<'a>(ctx: &'a Context<'_>) -> &'a Arc<crate::prefs::Prefs> {
    ctx.data_unchecked::<Arc<crate::prefs::Prefs>>()
}

async fn host_call(ctx: &Context<'_>, method: &str, params: Value) -> async_graphql::Result<Value> {
    ctx.data_unchecked::<Arc<Host>>()
        .call(method, params)
        .await
        .map_err(async_graphql::Error::new)
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/public_audio.rs"]
mod tests;
