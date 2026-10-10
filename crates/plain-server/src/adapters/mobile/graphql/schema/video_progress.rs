use async_graphql::{Context, ID, Object, SimpleObject};
use std::sync::Arc;
use {
    crate::graphql::types::{Instant, Long},
    plain_rs::db::{Db, VideoPlayProgressRow},
};

#[derive(SimpleObject)]
pub struct VideoPlayProgress {
    pub media_id: ID,
    pub position_ms: Long,
    pub updated_at: Instant,
}
fn record(row: VideoPlayProgressRow) -> async_graphql::Result<VideoPlayProgress> {
    Ok(VideoPlayProgress {
        media_id: row.media_id.into(),
        position_ms: Long(row.position_ms),
        updated_at: crate::graphql::shared::content_common::instant(&row.updated_at)?,
    })
}
#[derive(Default)]
pub struct VideoProgressQuery;
#[Object]
impl VideoProgressQuery {
    async fn video_play_progress(
        &self,
        ctx: &Context<'_>,
        media_id: ID,
    ) -> async_graphql::Result<Option<VideoPlayProgress>> {
        ctx.data::<Arc<Db>>()?
            .video_progress_get(media_id.as_str())?
            .map(record)
            .transpose()
    }
    async fn recent_video_play_progress(
        &self,
        ctx: &Context<'_>,
        since: Instant,
    ) -> async_graphql::Result<Vec<VideoPlayProgress>> {
        plain_rs::video_progress::recent(ctx.data::<Arc<Db>>()?, since.0)?
            .into_iter()
            .map(record)
            .collect()
    }
}
#[derive(Default)]
pub struct VideoProgressMutation;
#[Object]
impl VideoProgressMutation {
    async fn save_video_play_progress(
        &self,
        ctx: &Context<'_>,
        media_id: ID,
        position_ms: Long,
    ) -> async_graphql::Result<VideoPlayProgress> {
        record(plain_rs::video_progress::save(
            ctx.data::<Arc<Db>>()?,
            media_id.as_str(),
            position_ms.0,
        )?)
    }
    async fn delete_video_play_progress(
        &self,
        ctx: &Context<'_>,
        media_id: ID,
    ) -> async_graphql::Result<bool> {
        plain_rs::video_progress::delete(ctx.data::<Arc<Db>>()?, media_id.as_str())?;
        Ok(true)
    }
}
