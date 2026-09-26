//! Audio playback queue, user playlists and play history — the shared
//! plain-rs `library` core (`local_library.db`), the same behavior the NAS
//! server runs. The desktop local server has no media index yet, so the
//! LIBRARY source resolves to zero tracks (playAllAudios returns null);
//! the manual queue, playlists and history work in full.

use async_graphql::{Context, ID, Object};
use plain_rs::library::audio_queue::{self, AudioTrack};
use std::sync::Arc;

use crate::local::db::LibraryDb;
use crate::local::enums::MediaPlayMode;
use crate::local::graphql::context::AppCtx;
use crate::local::graphql::schema::types::{
    AudioItem, AudioPlayHistory, AudioPlayback, AudioPlaylist,
};

fn track_to_gql(a: AudioTrack) -> AudioItem {
    AudioItem {
        title: a.title,
        artist: a.artist,
        path: a.path,
        duration_ms: a.duration_secs * 1000,
    }
}

fn playlist_to_gql((pl, count): (plain_rs::library::db::Playlist, usize)) -> AudioPlaylist {
    AudioPlaylist {
        id: pl.id,
        name: pl.name,
        item_count: count.min(i32::MAX as usize) as i32,
        created_at: pl.created_at,
        updated_at: pl.updated_at,
    }
}

#[derive(Default)]
pub struct AudioQueueQuery;

#[Object]
impl AudioQueueQuery {
    /// The active playback queue (manual items + context), paginated.
    /// `query` is the shared DSL; its `text:` field filters the page.
    async fn audio_queue_items(
        &self,
        ctx: &Context<'_>,
        offset: i32,
        limit: i32,
        query: String,
    ) -> Vec<AudioItem> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let text = text_of(&query);
        let mut lib = audio_queue::NoLibrary;
        audio_queue::queue_page(&c.library, &mut lib, offset as i64, limit as i64, &text)
            .unwrap_or_default()
            .into_iter()
            .map(track_to_gql)
            .collect()
    }

    /// Total tracks in the active playback queue.
    async fn audio_queue_item_count(&self, ctx: &Context<'_>) -> i32 {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let mut lib = audio_queue::NoLibrary;
        audio_queue::queue_total(&c.library, &mut lib)
            .unwrap_or(0)
            .min(i32::MAX as usize) as i32
    }

    /// Player state: play mode preference and the current queue track
    /// path (null = idle). The desktop backend has no transport — audio
    /// renders on the client — so isPlaying/positionMs serve idle values.
    async fn audio_playback(&self, ctx: &Context<'_>) -> AudioPlayback {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let current = audio_queue::get_audio_current(&c.library);
        AudioPlayback {
            current_path: (!current.is_empty()).then_some(current),
            mode: media_play_mode_of(&c.library),
            is_playing: false,
            position_ms: 0,
        }
    }

    /// All user playlists, most recently updated first.
    async fn audio_playlists(&self, ctx: &Context<'_>) -> Vec<AudioPlaylist> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        audio_queue::playlists(&c.library)
            .into_iter()
            .map(playlist_to_gql)
            .collect()
    }

    /// One playlist's tracks, position order, paginated.
    async fn audio_playlist_items(
        &self,
        ctx: &Context<'_>,
        id: ID,
        offset: i32,
        limit: i32,
        query: String,
    ) -> Vec<AudioItem> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        audio_queue::playlist_items_page(
            &c.library,
            id.as_ref(),
            offset as i64,
            limit as i64,
            text_of(&query).as_str(),
        )
        .into_iter()
        .map(track_to_gql)
        .collect()
    }

    /// Track count of one playlist.
    async fn audio_playlist_item_count(&self, ctx: &Context<'_>, id: ID) -> i32 {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        audio_queue::playlist_item_count(&c.library, id.as_ref()).min(i32::MAX as usize) as i32
    }

    /// Recently played tracks, newest first.
    async fn audio_play_history(
        &self,
        ctx: &Context<'_>,
        offset: i32,
        limit: i32,
        query: String,
    ) -> Vec<AudioPlayHistory> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        audio_queue::history_page(
            &c.library,
            offset as i64,
            limit as i64,
            text_of(&query).as_str(),
        )
        .into_iter()
        .map(|h| AudioPlayHistory {
            path: h.path,
            title: h.title,
            artist: h.artist,
            duration_ms: h.duration_secs * 1000,
            play_count: h.play_count,
            played_at: h.played_at,
        })
        .collect()
    }
}

#[derive(Default)]
pub struct AudioQueueMutation;

#[Object]
impl AudioQueueMutation {
    /// Play the given track: mark it current, enqueue it in the manual
    /// queue when missing, and record the play.
    async fn play_audio(&self, ctx: &Context<'_>, path: String) -> AudioItem {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let track = AudioTrack::from_path_stem(&path);
        audio_queue::enqueue(&c.library, std::slice::from_ref(&track), false);
        audio_queue::on_playing(
            &c.library,
            &track.path,
            &track.title,
            &track.artist,
            track.duration_secs,
        );
        track_to_gql(track)
    }

    /// Persist the playback mode preference (REPEAT/REPEAT_ONE/SHUFFLE).
    async fn update_audio_play_mode(&self, ctx: &Context<'_>, mode: MediaPlayMode) -> bool {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let mode_str = match mode {
            MediaPlayMode::Repeat => "REPEAT",
            MediaPlayMode::RepeatOne => "REPEAT_ONE",
            MediaPlayMode::Shuffle => "SHUFFLE",
        };
        audio_queue::save_audio_mode(&c.library, mode_str);
        true
    }

    /// Reset the source, the manual queue and the current track.
    async fn clear_audio_queue(&self, ctx: &Context<'_>) -> bool {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        audio_queue::save_audio_current(&c.library, "");
        audio_queue::clear_queue(&c.library);
        true
    }

    /// Remove a track from the manual queue.
    async fn remove_audio_from_queue(&self, ctx: &Context<'_>, path: String) -> bool {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        audio_queue::remove_queued(&c.library, &path);
        true
    }

    /// Add up to 1000 tracks matching `query` to the manual queue. The
    /// desktop local server has no media index — only the explicit
    /// `ids:`…-style paths would be meaningful, and the audio page that
    /// builds such queries is empty here, so this is a no-op returning
    /// `true` until a library index exists.
    async fn add_audios_to_queue(&self, _ctx: &Context<'_>, _query: String) -> bool {
        true
    }

    /// Drag & drop reorder of the manual queue; unknown paths keep their
    /// order at the end.
    async fn reorder_audio_queue(&self, ctx: &Context<'_>, paths: Vec<String>) -> bool {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        audio_queue::reorder_queued(&c.library, &paths);
        true
    }

    // ----- User playlists -----

    /// Create an empty user playlist and return it.
    async fn create_audio_playlist(&self, ctx: &Context<'_>, name: String) -> AudioPlaylist {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        playlist_to_gql((audio_queue::create_playlist(&c.library, &name), 0))
    }

    /// Update a playlist's name; returns the updated playlist.
    async fn update_audio_playlist(
        &self,
        ctx: &Context<'_>,
        id: ID,
        name: String,
    ) -> async_graphql::Result<AudioPlaylist> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        audio_queue::rename_playlist(&c.library, id.as_ref(), &name);
        let pl = audio_queue::playlist_by_id(&c.library, id.as_ref())
            .ok_or_else(|| async_graphql::Error::new(format!("Playlist {} not found", id.0)))?;
        let count = audio_queue::playlist_item_count(&c.library, id.as_ref());
        Ok(playlist_to_gql((pl, count)))
    }

    /// Delete a playlist; its items go with it.
    async fn delete_audio_playlist(&self, ctx: &Context<'_>, id: ID) -> bool {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        audio_queue::delete_playlist(&c.library, id.as_ref());
        true
    }

    /// Add tracks (by path) to a playlist; duplicates are ignored.
    async fn add_audio_playlist_items(
        &self,
        ctx: &Context<'_>,
        id: ID,
        paths: Vec<String>,
    ) -> bool {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let items: Vec<AudioTrack> = paths
            .iter()
            .map(|p| AudioTrack::from_path_stem(p))
            .collect();
        audio_queue::add_playlist_items(&c.library, id.as_ref(), &items);
        true
    }

    /// Remove one track (by path) from a playlist.
    async fn remove_audio_playlist_item(&self, ctx: &Context<'_>, id: ID, path: String) -> bool {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        audio_queue::remove_playlist_item(&c.library, id.as_ref(), &path);
        true
    }

    /// Play a user playlist: make it the playback source and resolve the
    /// track to start with (random one when shuffling).
    async fn play_audio_playlist(
        &self,
        ctx: &Context<'_>,
        id: ID,
        path: Option<String>,
        shuffle: bool,
    ) -> Option<AudioItem> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let start = audio_queue::set_playlist_source(&c.library, id.as_ref(), path.as_deref());
        let track = if shuffle {
            match start {
                Some(_) => {
                    let mut lib = audio_queue::NoLibrary;
                    audio_queue::resolve_next(&c.library, &mut lib, true, true)
                        .ok()
                        .flatten()
                }
                None => None,
            }
        } else {
            start
        };
        track.map(track_to_gql)
    }

    /// Queue the whole audio library and start playback. The desktop
    /// local server has no media index — the library is empty, so this
    /// returns null.
    async fn play_all_audios(&self, ctx: &Context<'_>, shuffle: bool) -> Option<AudioItem> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let mut lib = audio_queue::NoLibrary;
        audio_queue::set_library_source(&c.library, &mut lib, None, shuffle)
            .ok()
            .flatten()
            .map(track_to_gql)
    }
}

/// The DSL `text:` field of a page query — extracted with the shared
/// parser (plain-rs `utils::search_dsl`), same as the NAS server.
fn text_of(query: &str) -> String {
    plain_rs::utils::search_dsl::field_value(query, "text").unwrap_or_default()
}

/// Parse the stored play-mode name into the GraphQL enum (REPEAT default).
fn media_play_mode_of(library: &LibraryDb) -> MediaPlayMode {
    match audio_queue::get_audio_mode(library).as_str() {
        "REPEAT_ONE" => MediaPlayMode::RepeatOne,
        "SHUFFLE" => MediaPlayMode::Shuffle,
        _ => MediaPlayMode::Repeat,
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/local/graphql/schema/audio_queue.rs"]
mod tests;
