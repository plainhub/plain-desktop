//! GraphQL mutations for the peer file download queue.
//!
//! Mirrors plain-app's `DownloadQueue.addDownloadTask` /
//! `pauseDownload` / `resumeDownload` / `retryDownload`. The actual
//! download runs in a background tokio task (`crate::api::download`);
//! progress is streamed back to the web client via `WS_DOWNLOAD_PROGRESS`.

use async_graphql::{Context, Object, Result as GqlResult};
use std::sync::Arc;

use super::super::context::AppCtx;
use crate::api::download;

#[derive(Default)]
pub struct DownloadMutation;

#[Object]
impl DownloadMutation {
    /// Start downloading all `fsid:` files in the given message from the
    /// given peer. Returns `false` if a download is already in flight for
    /// that message. Progress is reported via `WS_DOWNLOAD_PROGRESS`.
    async fn download_peer_file(
        &self,
        ctx: &Context<'_>,
        message_id: String,
        peer_id: String,
    ) -> GqlResult<bool> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>().clone();
        let ok = download::start_download(c, message_id, peer_id).await;
        Ok(ok)
    }

    /// Pause an in-flight download. Returns `false` if no active task
    /// exists for the message.
    async fn pause_download(&self, ctx: &Context<'_>, message_id: String) -> GqlResult<bool> {
        let _ = ctx.data_unchecked::<Arc<AppCtx>>();
        Ok(download::pause_download(&message_id).await)
    }

    /// Resume a paused download. The peer id is required because the
    /// manager does not cache it between pause/resume cycles.
    async fn resume_download(
        &self,
        ctx: &Context<'_>,
        message_id: String,
        peer_id: String,
    ) -> GqlResult<bool> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>().clone();
        Ok(download::resume_download(c, message_id, peer_id).await)
    }

    /// Retry a failed download.
    async fn retry_download(
        &self,
        ctx: &Context<'_>,
        message_id: String,
        peer_id: String,
    ) -> GqlResult<bool> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>().clone();
        Ok(download::retry_download(c, message_id, peer_id).await)
    }
}
