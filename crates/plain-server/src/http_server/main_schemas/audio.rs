//! Audio playback queue, user playlists and play history — the shared
//! plain-rs `library` core (`the shared SQLite database`), the same behavior the NAS
//! server runs. The desktop local server has no media index yet, so the
//! LIBRARY source resolves to zero tracks (playAllAudios returns null);
//! the manual queue, playlists and history work in full.

use super::types::Long;
use crate::library::audio_queue::{self, AudioTrack};
use async_graphql::{Context, ID, Object};
use std::sync::Arc;

use crate::api::context::AppCtx;
use crate::api::enums::MediaPlayMode;
use crate::http_server::main_schemas::types::{
    AudioItem, AudioPlayHistory, AudioPlayback, AudioPlaylist,
};

fn track_to_gql(a: AudioTrack) -> AudioItem {
    AudioItem {
        title: a.title,
        artist: a.artist,
        path: a.path,
        duration_ms: Long(a.duration_ms),
    }
}

fn playlist_to_gql((pl, count): (crate::db::Playlist, usize)) -> AudioPlaylist {
    AudioPlaylist {
        id: pl.id.into(),
        name: pl.name,
        item_count: count.min(i32::MAX as usize) as i32,
        created_at: pl.created_at,
        updated_at: pl.updated_at,
    }
}

#[derive(Default)]
pub struct AudioQuery;

#[Object]
impl AudioQuery {
    /// The active playback queue (manual items + context), paginated.
    /// `query` is the shared DSL; its `text:` field filters the page.
    async fn audio_queue_items(
        &self,
        ctx: &Context<'_>,
        offset: i32,
        limit: i32,
        query: String,
    ) -> async_graphql::Result<Vec<AudioItem>> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let text = text_of(&query);
        let mut lib = audio_queue::NoLibrary;
        Ok(
            audio_queue::queue_page(&c.db, &mut lib, offset as i64, limit as i64, &text)?
                .into_iter()
                .map(track_to_gql)
                .collect(),
        )
    }

    /// Total tracks in the active playback queue.
    async fn audio_queue_item_count(&self, ctx: &Context<'_>) -> async_graphql::Result<i32> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let mut lib = audio_queue::NoLibrary;
        Ok(audio_queue::queue_total(&c.db, &mut lib)?.min(i32::MAX as usize) as i32)
    }

    /// Player state: play mode preference and the current queue track
    /// path (null = idle). The desktop backend has no transport — audio
    /// renders on the client — so isPlaying/positionMs serve idle values.
    async fn audio_playback(&self, ctx: &Context<'_>) -> async_graphql::Result<AudioPlayback> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let current = audio_queue::get_audio_current(&c.db)?;
        Ok(AudioPlayback {
            current_path: (!current.is_empty()).then_some(current),
            mode: media_play_mode_of(&c.prefs)?,
            is_playing: false,
            position_ms: Long(0),
        })
    }

    /// All user playlists, most recently updated first.
    async fn audio_playlists(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<AudioPlaylist>> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        Ok(audio_queue::playlists(&c.db)?
            .into_iter()
            .map(playlist_to_gql)
            .collect())
    }

    /// One playlist's tracks, position order, paginated.
    async fn audio_playlist_items(
        &self,
        ctx: &Context<'_>,
        id: ID,
        offset: i32,
        limit: i32,
        query: String,
    ) -> async_graphql::Result<Vec<AudioItem>> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        Ok(audio_queue::playlist_items_page(
            &c.db,
            id.as_ref(),
            offset as i64,
            limit as i64,
            text_of(&query).as_str(),
        )?
        .into_iter()
        .map(track_to_gql)
        .collect())
    }

    /// Track count of one playlist.
    async fn audio_playlist_item_count(
        &self,
        ctx: &Context<'_>,
        id: ID,
    ) -> async_graphql::Result<i32> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        Ok(audio_queue::playlist_item_count(&c.db, id.as_ref())?.min(i32::MAX as usize) as i32)
    }

    /// Recently played tracks, newest first.
    async fn audio_play_history(
        &self,
        ctx: &Context<'_>,
        offset: i32,
        limit: i32,
        query: String,
    ) -> async_graphql::Result<Vec<AudioPlayHistory>> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        Ok(
            audio_queue::history_page(
                &c.db,
                offset as i64,
                limit as i64,
                text_of(&query).as_str(),
            )?
            .into_iter()
            .map(|h| AudioPlayHistory {
                path: h.path,
                title: h.title,
                artist: h.artist,
                duration_ms: Long(h.duration_ms),
                play_count: h.play_count,
                played_at: h.played_at,
            })
            .collect(),
        )
    }
}

#[derive(Default)]
pub struct AudioMutation;

#[Object]
impl AudioMutation {
    /// Mark the track current and record playback without changing queue order.
    async fn play_audio(
        &self,
        ctx: &Context<'_>,
        path: String,
    ) -> async_graphql::Result<AudioItem> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let track = AudioTrack::from_path_stem(&path);
        audio_queue::on_playing(
            &c.db,
            &track.path,
            &track.title,
            &track.artist,
            track.duration_ms,
        )?;
        Ok(track_to_gql(track))
    }

    /// Persist the playback mode preference (REPEAT/REPEAT_ONE/SHUFFLE).
    async fn update_audio_play_mode(
        &self,
        ctx: &Context<'_>,
        mode: MediaPlayMode,
    ) -> async_graphql::Result<bool> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let mode_str = match mode {
            MediaPlayMode::Repeat => "REPEAT",
            MediaPlayMode::RepeatOne => "REPEAT_ONE",
            MediaPlayMode::Shuffle => "SHUFFLE",
        };
        audio_queue::save_audio_mode(&c.prefs, mode_str)
            .map(|_| true)
            .map_err(|error| async_graphql::Error::new(error.to_string()))
    }

    /// Reset the source, the manual queue and the current track.
    async fn clear_audio_queue(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        audio_queue::clear_queue(&c.db)?;
        Ok(true)
    }

    /// Remove a track from the manual queue.
    async fn remove_audio_from_queue(
        &self,
        ctx: &Context<'_>,
        path: String,
    ) -> async_graphql::Result<bool> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        audio_queue::remove_queued(&c.db, &path)?;
        Ok(true)
    }

    /// Add up to 1000 tracks matching `query` to the manual queue. The
    /// desktop local server has no media index — only the explicit
    /// `ids:`…-style paths would be meaningful, and the audio page that
    /// builds such queries is empty here, so this is a no-op returning
    /// `true` until a library index exists.
    async fn add_audios_to_queue(
        &self,
        _ctx: &Context<'_>,
        _query: String,
    ) -> async_graphql::Result<bool> {
        Ok(true)
    }

    /// Drag & drop reorder of the manual queue; unknown paths keep their
    /// order at the end.
    async fn reorder_audio_queue(
        &self,
        ctx: &Context<'_>,
        paths: Vec<String>,
    ) -> async_graphql::Result<bool> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        audio_queue::reorder_queued(&c.db, &paths)?;
        Ok(true)
    }

    // ----- User playlists -----

    /// Create an empty user playlist and return it.
    async fn create_audio_playlist(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> async_graphql::Result<AudioPlaylist> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        Ok(playlist_to_gql((
            audio_queue::create_playlist(&c.db, &name)?,
            0,
        )))
    }

    /// Update a playlist's name; returns the updated playlist.
    async fn update_audio_playlist(
        &self,
        ctx: &Context<'_>,
        id: ID,
        name: String,
    ) -> async_graphql::Result<AudioPlaylist> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        audio_queue::rename_playlist(&c.db, id.as_ref(), &name)?;
        let pl = audio_queue::playlist_by_id(&c.db, id.as_ref())?
            .ok_or_else(|| async_graphql::Error::new(format!("Playlist {} not found", id.0)))?;
        let count = audio_queue::playlist_item_count(&c.db, id.as_ref())?;
        Ok(playlist_to_gql((pl, count)))
    }

    /// Delete a playlist; its items go with it.
    async fn delete_audio_playlist(
        &self,
        ctx: &Context<'_>,
        id: ID,
    ) -> async_graphql::Result<bool> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        audio_queue::delete_playlist(&c.db, id.as_ref())?;
        Ok(true)
    }

    /// Add tracks (by path) to a playlist; duplicates are ignored.
    async fn add_audio_playlist_items(
        &self,
        ctx: &Context<'_>,
        id: ID,
        paths: Vec<String>,
    ) -> async_graphql::Result<bool> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let items: Vec<AudioTrack> = paths
            .iter()
            .map(|p| AudioTrack::from_path_stem(p))
            .collect();
        audio_queue::add_playlist_items(&c.db, id.as_ref(), &items)?;
        Ok(true)
    }

    /// Remove one track (by path) from a playlist.
    async fn remove_audio_playlist_item(
        &self,
        ctx: &Context<'_>,
        id: ID,
        path: String,
    ) -> async_graphql::Result<bool> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        audio_queue::remove_playlist_item(&c.db, id.as_ref(), &path)?;
        Ok(true)
    }

    /// Play a user playlist: make it the playback source and resolve the
    /// track to start with (random one when shuffling).
    async fn play_audio_playlist(
        &self,
        ctx: &Context<'_>,
        id: ID,
        path: Option<String>,
        shuffle: bool,
    ) -> async_graphql::Result<Option<AudioItem>> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let start = audio_queue::set_playlist_source(&c.db, id.as_ref(), path.as_deref())?;
        let track = if shuffle {
            match start {
                Some(_) => {
                    let mut lib = audio_queue::NoLibrary;
                    audio_queue::resolve_next(&c.db, &mut lib, true, true)?
                }
                None => None,
            }
        } else {
            start
        };
        Ok(track.map(track_to_gql))
    }

    /// Queue the whole audio library and start playback. The desktop
    /// local server has no media index — the library is empty, so this
    /// returns null.
    async fn play_all_audios(
        &self,
        ctx: &Context<'_>,
        shuffle: bool,
    ) -> async_graphql::Result<Option<AudioItem>> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let mut lib = audio_queue::NoLibrary;
        Ok(
            audio_queue::set_library_source(&c.db, &mut lib, None, shuffle, "DATE_DESC")?
                .map(track_to_gql),
        )
    }
}

/// The DSL `text:` field of a page query — extracted with the shared
/// parser (plain-rs `utils::search_dsl`), same as the NAS server.
fn text_of(query: &str) -> String {
    crate::utils::search_dsl::field_value(query, "text").unwrap_or_default()
}

fn media_play_mode_of(prefs: &crate::prefs::Prefs) -> async_graphql::Result<MediaPlayMode> {
    Ok(audio_queue::play_mode(prefs)?)
}

#[cfg(test)]
#[path = "../../../tests/unit/api/schema/audio_queue.rs"]
mod tests;
