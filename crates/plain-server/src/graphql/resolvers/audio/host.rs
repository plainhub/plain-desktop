use crate::graphql::resolvers::audio::public::AudioItem;
use crate::graphql::types::{Instant, Long};
use crate::services::audio::host::Audio;
use async_graphql::{Context, ID, InputObject, Object, SimpleObject};
use plain_rs::db::PlaylistItem;
use plain_rs::db::QueueSource;
use plain_rs::media::audio::queue as domain;
use plain_rs::media::audio::queue::AudioTrack;
use std::sync::Arc;

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
pub struct AudioHostSource {
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
                Ok(plain_rs::db::audio_queue::all_queue_items(db)?
                    .into_iter()
                    .map(|r| r.path)
                    .collect())
            })
            .await
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
    ) -> async_graphql::Result<Option<AudioItem>> {
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
    ) -> async_graphql::Result<Option<AudioItem>> {
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
    ) -> async_graphql::Result<Option<AudioItem>> {
        Ok(audio(ctx)?
            .run(move |db, lib| domain::select_next(db, lib, is_next, shuffle))
            .await?
            .map(Into::into))
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