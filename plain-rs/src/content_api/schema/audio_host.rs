use crate::{
    content_api::audio::Audio,
    content_types::{Instant, Long},
    db::{PlayHistory, Playlist, PlaylistItem, QueueSource},
    library::audio_queue::{self as domain, AudioTrack},
};
use async_graphql::{Context, ID, InputObject, Object, SimpleObject};
use std::sync::Arc;

#[derive(SimpleObject)]
pub struct AudioHostTrack {
    path: String,
    title: String,
    artist: String,
    album_id: ID,
    duration_ms: Long,
}
impl From<AudioTrack> for AudioHostTrack {
    fn from(row: AudioTrack) -> Self {
        Self {
            path: row.path,
            title: row.title,
            artist: row.artist,
            album_id: row.album_id.into(),
            duration_ms: Long(row.duration_ms),
        }
    }
}
#[derive(InputObject)]
pub struct AudioHostTrackInput {
    path: String,
    title: String,
    artist: String,
    album_id: ID,
    duration_ms: Long,
}
impl From<AudioHostTrackInput> for AudioTrack {
    fn from(row: AudioHostTrackInput) -> Self {
        Self {
            path: row.path,
            title: row.title,
            artist: row.artist,
            album_id: row.album_id.to_string(),
            duration_ms: row.duration_ms.0,
        }
    }
}
#[derive(SimpleObject)]
struct AudioHostSource {
    source: String,
    playlist_id: ID,
    current_path: String,
    current_index: Long,
    sort_by: String,
}
impl From<QueueSource> for AudioHostSource {
    fn from(row: QueueSource) -> Self {
        Self {
            source: row.source.as_str().into(),
            playlist_id: row.playlist_id.into(),
            current_path: row.current_path,
            current_index: Long(row.current_index),
            sort_by: row.sort_by,
        }
    }
}
#[derive(SimpleObject)]
struct AudioHostPlaylist {
    id: ID,
    name: String,
    item_count: i32,
    created_at: Instant,
    updated_at: Instant,
}
fn playlist(row: Playlist, count: usize) -> async_graphql::Result<AudioHostPlaylist> {
    Ok(AudioHostPlaylist {
        id: row.id.into(),
        name: row.name,
        item_count: count.try_into()?,
        created_at: super::content_common::instant(&row.created_at)?,
        updated_at: super::content_common::instant(&row.updated_at)?,
    })
}
#[derive(SimpleObject)]
struct AudioHostPlaylistItem {
    id: ID,
    playlist_id: ID,
    audio_path: String,
    title: String,
    artist: String,
    album_id: ID,
    duration_ms: Long,
    sort_order: Long,
    added_at: Instant,
}
fn item(row: PlaylistItem) -> async_graphql::Result<AudioHostPlaylistItem> {
    Ok(AudioHostPlaylistItem {
        id: row.id.into(),
        playlist_id: row.playlist_id.into(),
        audio_path: row.audio_path,
        title: row.title,
        artist: row.artist,
        album_id: row.album_id.into(),
        duration_ms: Long(row.duration_ms),
        sort_order: Long(row.sort_order),
        added_at: super::content_common::instant(&row.added_at)?,
    })
}
#[derive(SimpleObject)]
struct AudioHostHistory {
    path: String,
    title: String,
    artist: String,
    duration_ms: Long,
    play_count: Long,
    played_at: Instant,
}
fn history(row: PlayHistory) -> async_graphql::Result<AudioHostHistory> {
    Ok(AudioHostHistory {
        path: row.path,
        title: row.title,
        artist: row.artist,
        duration_ms: Long(row.duration_ms),
        play_count: Long(row.play_count),
        played_at: super::content_common::instant(&row.played_at)?,
    })
}
#[derive(SimpleObject)]
struct AudioArtistPlayCount {
    artist: String,
    count: Long,
}
fn audio<'a>(ctx: &'a Context<'_>) -> async_graphql::Result<&'a Arc<Audio>> {
    ctx.data::<Arc<Audio>>()
}
#[derive(Default)]
pub struct AudioHostQuery;
#[Object]
impl AudioHostQuery {
    async fn audio_source(&self, ctx: &Context<'_>) -> async_graphql::Result<AudioHostSource> {
        Ok(audio(ctx)?.run(|db, _| domain::source(db)).await?.into())
    }
    async fn audio_queued_paths(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<String>> {
        audio(ctx)?
            .run(|db, _| {
                Ok(crate::db::audio_queue::all_queue_items(db)?
                    .into_iter()
                    .map(|r| r.path)
                    .collect())
            })
            .await
    }
    async fn audio_host_queue_items(
        &self,
        ctx: &Context<'_>,
        offset: i32,
        limit: i32,
        query: String,
    ) -> async_graphql::Result<Vec<AudioHostTrack>> {
        Ok(audio(ctx)?
            .run(move |db, lib| domain::queue_page(db, lib, offset as i64, limit as i64, &query))
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    }
    async fn audio_host_queue_count(&self, ctx: &Context<'_>) -> async_graphql::Result<i32> {
        Ok(audio(ctx)?
            .run(|db, lib| domain::queue_total(db, lib))
            .await?
            .try_into()?)
    }
    async fn audio_host_playlists(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<AudioHostPlaylist>> {
        audio(ctx)?
            .run(|db, _| domain::playlists(db))
            .await?
            .into_iter()
            .map(|(r, c)| playlist(r, c))
            .collect()
    }
    async fn audio_host_playlist(
        &self,
        ctx: &Context<'_>,
        id: ID,
    ) -> async_graphql::Result<Option<AudioHostPlaylist>> {
        audio(ctx)?
            .run(move |db, _| {
                domain::playlist_by_id(db, id.as_str())?
                    .map(|r| Ok((r, domain::playlist_item_count(db, id.as_str())?)))
                    .transpose()
            })
            .await?
            .map(|(r, c)| playlist(r, c))
            .transpose()
    }
    async fn audio_host_playlist_items(
        &self,
        ctx: &Context<'_>,
        id: ID,
        offset: i32,
        limit: i32,
        query: String,
    ) -> async_graphql::Result<Vec<AudioHostPlaylistItem>> {
        audio(ctx)?
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
            .map(item)
            .collect()
    }
    async fn audio_host_playlist_item_count(
        &self,
        ctx: &Context<'_>,
        id: ID,
    ) -> async_graphql::Result<i32> {
        Ok(audio(ctx)?
            .run(move |db, _| domain::playlist_item_count(db, id.as_str()))
            .await?
            .try_into()?)
    }
    async fn audio_host_history(
        &self,
        ctx: &Context<'_>,
        offset: i32,
        limit: i32,
        query: String,
    ) -> async_graphql::Result<Vec<AudioHostHistory>> {
        audio(ctx)?
            .run(move |db, _| domain::history_page(db, offset as i64, limit as i64, &query))
            .await?
            .into_iter()
            .map(history)
            .collect()
    }
    async fn audio_artist_play_counts(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<AudioArtistPlayCount>> {
        Ok(audio(ctx)?
            .run(|db, _| domain::artist_play_counts(db))
            .await?
            .into_iter()
            .map(|(artist, count)| AudioArtistPlayCount {
                artist,
                count: Long(count),
            })
            .collect())
    }
}
#[derive(Default)]
pub struct AudioHostMutation;
#[Object]
impl AudioHostMutation {
    async fn audio_host_enqueue(
        &self,
        ctx: &Context<'_>,
        items: Vec<AudioHostTrackInput>,
        play_next: bool,
    ) -> async_graphql::Result<bool> {
        audio(ctx)?
            .run(move |db, _| {
                domain::enqueue(
                    db,
                    &items.into_iter().map(Into::into).collect::<Vec<_>>(),
                    play_next,
                )
            })
            .await?;
        Ok(true)
    }
    async fn audio_host_remove_queued(
        &self,
        ctx: &Context<'_>,
        path: String,
    ) -> async_graphql::Result<bool> {
        audio(ctx)?
            .run(move |db, _| domain::remove_queued(db, &path))
            .await?;
        Ok(true)
    }
    async fn audio_host_reorder(
        &self,
        ctx: &Context<'_>,
        paths: Vec<String>,
    ) -> async_graphql::Result<bool> {
        audio(ctx)?
            .run(move |db, _| domain::reorder_queued(db, &paths))
            .await?;
        Ok(true)
    }
    async fn audio_host_move_queued(
        &self,
        ctx: &Context<'_>,
        from: i32,
        to: i32,
    ) -> async_graphql::Result<bool> {
        audio(ctx)?
            .run(move |db, _| domain::move_queued(db, from, to))
            .await?;
        Ok(true)
    }
    async fn audio_host_clear(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        audio(ctx)?.run(|db, _| domain::clear_queue(db)).await?;
        Ok(true)
    }
    async fn audio_host_remove_paths(
        &self,
        ctx: &Context<'_>,
        paths: Vec<String>,
    ) -> async_graphql::Result<bool> {
        audio(ctx)?
            .run(move |db, _| domain::remove_paths(db, &paths))
            .await?;
        Ok(true)
    }
    async fn audio_host_set_current(
        &self,
        ctx: &Context<'_>,
        path: String,
    ) -> async_graphql::Result<bool> {
        audio(ctx)?
            .run(move |db, _| domain::save_audio_current(db, &path))
            .await?;
        Ok(true)
    }
    async fn audio_host_on_playing(
        &self,
        ctx: &Context<'_>,
        path: String,
        title: String,
        artist: String,
        duration_ms: Long,
    ) -> async_graphql::Result<bool> {
        audio(ctx)?
            .run(move |db, _| domain::on_playing(db, &path, &title, &artist, duration_ms.0))
            .await?;
        Ok(true)
    }
    async fn audio_host_set_playlist_source(
        &self,
        ctx: &Context<'_>,
        id: ID,
        start_path: Option<String>,
    ) -> async_graphql::Result<Option<AudioHostTrack>> {
        Ok(audio(ctx)?
            .run(move |db, _| {
                domain::select_playlist_source(db, id.as_str(), start_path.as_deref())
            })
            .await?
            .map(Into::into))
    }
    async fn audio_host_set_library_source(
        &self,
        ctx: &Context<'_>,
        start_path: Option<String>,
        shuffle: bool,
        sort_by: String,
    ) -> async_graphql::Result<Option<AudioHostTrack>> {
        Ok(audio(ctx)?
            .run(move |db, lib| {
                domain::select_library_source(db, lib, start_path.as_deref(), shuffle, &sort_by)
            })
            .await?
            .map(Into::into))
    }
    async fn audio_host_resolve_next(
        &self,
        ctx: &Context<'_>,
        is_next: bool,
        shuffle: bool,
    ) -> async_graphql::Result<Option<AudioHostTrack>> {
        Ok(audio(ctx)?
            .run(move |db, lib| domain::select_next(db, lib, is_next, shuffle))
            .await?
            .map(Into::into))
    }
    async fn audio_host_create_playlist(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> async_graphql::Result<AudioHostPlaylist> {
        playlist(
            audio(ctx)?
                .run(move |db, _| domain::create_playlist(db, &name))
                .await?,
            0,
        )
    }
    async fn audio_host_rename_playlist(
        &self,
        ctx: &Context<'_>,
        id: ID,
        name: String,
    ) -> async_graphql::Result<bool> {
        audio(ctx)?
            .run(move |db, _| domain::rename_playlist(db, id.as_str(), &name))
            .await?;
        Ok(true)
    }
    async fn audio_host_delete_playlist(
        &self,
        ctx: &Context<'_>,
        id: ID,
    ) -> async_graphql::Result<bool> {
        audio(ctx)?
            .run(move |db, _| domain::delete_playlist(db, id.as_str()))
            .await?;
        Ok(true)
    }
    async fn audio_host_add_playlist_items(
        &self,
        ctx: &Context<'_>,
        id: ID,
        items: Vec<AudioHostTrackInput>,
    ) -> async_graphql::Result<i32> {
        Ok(audio(ctx)?
            .run(move |db, _| {
                domain::add_playlist_items(
                    db,
                    id.as_str(),
                    &items.into_iter().map(Into::into).collect::<Vec<_>>(),
                )
            })
            .await?
            .try_into()?)
    }
    async fn audio_host_remove_playlist_items(
        &self,
        ctx: &Context<'_>,
        id: ID,
        paths: Vec<String>,
    ) -> async_graphql::Result<bool> {
        audio(ctx)?
            .run(move |db, _| domain::remove_playlist_items(db, id.as_str(), &paths))
            .await?;
        Ok(true)
    }
    async fn audio_host_record_history(
        &self,
        ctx: &Context<'_>,
        path: String,
        title: String,
        artist: String,
        duration_ms: Long,
    ) -> async_graphql::Result<bool> {
        audio(ctx)?
            .run(move |db, _| domain::record_history(db, &path, &title, &artist, duration_ms.0))
            .await?;
        Ok(true)
    }
}
