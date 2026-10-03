use crate::{
    content_api::image_index::{ImageIndex, Status},
    content_types::Long,
};
use async_graphql::{Context, ID, Object, Result, SimpleObject};
use std::sync::Arc;
#[derive(SimpleObject)]
struct ImageHostIndexStatus {
    version: Long,
    is_running: bool,
    total_images: Long,
    indexed_images: Long,
    skipped_images: Long,
    error_message: String,
}
fn status(s: Status) -> Result<ImageHostIndexStatus> {
    Ok(ImageHostIndexStatus {
        version: Long(s.version.try_into()?),
        is_running: s.is_running,
        total_images: Long(s.total_images.try_into()?),
        indexed_images: Long(s.indexed_images.try_into()?),
        skipped_images: Long(s.skipped_images.try_into()?),
        error_message: s.error_message,
    })
}
#[derive(Default)]
pub struct ImageIndexQuery;
#[Object]
impl ImageIndexQuery {
    async fn image_host_index_status(&self, ctx: &Context<'_>) -> Result<ImageHostIndexStatus> {
        status(ctx.data::<Arc<ImageIndex>>()?.status())
    }
}
#[derive(Default)]
pub struct ImageIndexMutation;
#[Object]
impl ImageIndexMutation {
    async fn image_host_start_index(
        &self,
        ctx: &Context<'_>,
        force: bool,
    ) -> Result<ImageHostIndexStatus> {
        status(ctx.data::<Arc<ImageIndex>>()?.start(force)?)
    }
    async fn image_host_index_selected(
        &self,
        ctx: &Context<'_>,
        ids: Vec<ID>,
    ) -> Result<ImageHostIndexStatus> {
        status(
            ctx.data::<Arc<ImageIndex>>()?
                .selected(ids.into_iter().map(|id| id.to_string()).collect())?,
        )
    }
    async fn image_host_cancel_index(&self, ctx: &Context<'_>) -> Result<ImageHostIndexStatus> {
        status(ctx.data::<Arc<ImageIndex>>()?.cancel()?)
    }
}
