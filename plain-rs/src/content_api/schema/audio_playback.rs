use crate::{
    content_api::audio::Audio,
    content_types::Long,
    library::{
        audio_commands::{self, Action},
        audio_playback::{self, Playback},
    },
};
use async_graphql::{Context, Enum, Object, SimpleObject};
use std::sync::Arc;

#[derive(SimpleObject)]
pub struct AudioPlayback {
    path: String,
    position_ms: Long,
    revision: Long,
}
impl From<Playback> for AudioPlayback {
    fn from(row: Playback) -> Self {
        Self {
            path: row.path,
            position_ms: Long(row.position_ms),
            revision: Long(row.revision),
        }
    }
}
#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum AudioCommand {
    Play,
    Pause,
    Seek,
    Next,
    Previous,
    Completed,
    Clear,
    Restart,
    SetSpeed,
}
impl From<AudioCommand> for Action {
    fn from(action: AudioCommand) -> Self {
        match action {
            AudioCommand::Play => Self::Play,
            AudioCommand::Pause => Self::Pause,
            AudioCommand::Seek => Self::Seek,
            AudioCommand::Next => Self::Next,
            AudioCommand::Previous => Self::Previous,
            AudioCommand::Completed => Self::Completed,
            AudioCommand::Clear => Self::Clear,
            AudioCommand::Restart => Self::Restart,
            AudioCommand::SetSpeed => Self::SetSpeed,
        }
    }
}
#[derive(Default)]
pub struct AudioPlaybackQuery;
#[Object]
impl AudioPlaybackQuery {
    async fn audio_playback(&self, ctx: &Context<'_>) -> async_graphql::Result<AudioPlayback> {
        Ok(ctx
            .data::<Arc<Audio>>()?
            .run(|db, _| audio_playback::snapshot(db))
            .await?
            .into())
    }
}
#[derive(Default)]
pub struct AudioPlaybackMutation;
#[Object]
impl AudioPlaybackMutation {
    async fn audio_report_started(
        &self,
        ctx: &Context<'_>,
        track: super::audio_host::AudioHostTrackInput,
        revision: Long,
    ) -> async_graphql::Result<bool> {
        ctx.data::<Arc<Audio>>()?
            .run(move |db, _| audio_playback::started(db, &track.into(), revision.0))
            .await
    }
    async fn audio_report_progress(
        &self,
        ctx: &Context<'_>,
        path: String,
        revision: Long,
        position_ms: Long,
    ) -> async_graphql::Result<bool> {
        ctx.data::<Arc<Audio>>()?
            .run(move |db, _| audio_playback::report(db, &path, revision.0, position_ms.0))
            .await
    }
    async fn audio_invalidate_engine(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<AudioPlayback> {
        Ok(ctx
            .data::<Arc<Audio>>()?
            .run(|db, _| audio_playback::invalidate(db))
            .await?
            .into())
    }
    async fn audio_command(
        &self,
        ctx: &Context<'_>,
        action: AudioCommand,
        position_ms: Long,
        speed: f32,
    ) -> async_graphql::Result<AudioPlayback> {
        let prefs = ctx.data::<Arc<crate::prefs::Prefs>>()?.clone();
        Ok(ctx
            .data::<Arc<Audio>>()?
            .run(move |db, engine| {
                audio_commands::command(db, &prefs, engine, action.into(), position_ms.0, speed)
            })
            .await?
            .into())
    }
    async fn audio_play_track(
        &self,
        ctx: &Context<'_>,
        track: super::audio_host::AudioHostTrackInput,
        enqueue: bool,
    ) -> async_graphql::Result<AudioPlayback> {
        let prefs = ctx.data::<Arc<crate::prefs::Prefs>>()?.clone();
        Ok(ctx
            .data::<Arc<Audio>>()?
            .run(move |db, engine| {
                audio_commands::play_track(db, &prefs, engine, track.into(), enqueue)
            })
            .await?
            .into())
    }
    async fn audio_play_path(
        &self,
        ctx: &Context<'_>,
        path: String,
    ) -> async_graphql::Result<AudioPlayback> {
        let prefs = ctx.data::<Arc<crate::prefs::Prefs>>()?.clone();
        Ok(ctx
            .data::<Arc<Audio>>()?
            .run(move |db, engine| audio_commands::play_path(db, &prefs, engine, &path))
            .await?
            .into())
    }
}
