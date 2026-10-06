//! Public `/graphql` image-search roots.
//!
//! The embedding model and the index itself are platform state; these roots
//! report it and forward the start/cancel/enable commands. Nothing here is
//! gated — plain-app lets any web client drive the index.

use super::public_facts::{flag, integer, text};
use crate::content_api::host::Host;
use async_graphql::{Context, Enum, Object, SimpleObject};
use std::sync::Arc;

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum ImageSearchStatusType {
    #[default]
    Unavailable,
    Downloading,
    Loading,
    Ready,
    Error,
}

impl ImageSearchStatusType {
    fn parse(value: &str) -> Self {
        match value {
            "DOWNLOADING" => Self::Downloading,
            "LOADING" => Self::Loading,
            "READY" => Self::Ready,
            "ERROR" => Self::Error,
            _ => Self::Unavailable,
        }
    }
}

#[derive(SimpleObject, Clone, Debug)]
pub struct ImageSearchStatus {
    pub status: ImageSearchStatusType,
    #[graphql(name = "downloadProgress")]
    pub download_progress: i32,
    #[graphql(name = "errorMessage")]
    pub error_message: String,
    #[graphql(name = "modelSize")]
    pub model_size: crate::content_types::Long,
    #[graphql(name = "modelDir")]
    pub model_dir: String,
    #[graphql(name = "isIndexing")]
    pub is_indexing: bool,
    #[graphql(name = "totalImages")]
    pub total_images: i32,
    #[graphql(name = "indexedImages")]
    pub indexed_images: i32,
}

#[derive(Default)]
pub struct ImageIndexQuery;

#[Object]
impl ImageIndexQuery {
    async fn image_search_status(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<ImageSearchStatus> {
        let status = ctx
            .data_unchecked::<Arc<Host>>()
            .call("systemImageSearchStatus", serde_json::json!({}))
            .await
            .map_err(|error| async_graphql::Error::new(error))?;
        Ok(ImageSearchStatus {
            status: ImageSearchStatusType::parse(&text(&status, "status")),
            download_progress: integer(&status, "downloadProgress") as i32,
            error_message: text(&status, "errorMessage"),
            model_size: crate::content_types::Long(integer(&status, "modelSize")),
            model_dir: text(&status, "modelDir"),
            is_indexing: flag(&status, "isIndexing"),
            total_images: integer(&status, "totalImages") as i32,
            indexed_images: integer(&status, "indexedImages") as i32,
        })
    }
}

#[derive(Default)]
pub struct ImageIndexMutation;

#[Object]
impl ImageIndexMutation {
    async fn enable_image_search(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        dispatch(ctx, "systemEnableImageSearch", false).await
    }

    async fn disable_image_search(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        dispatch(ctx, "systemDisableImageSearch", false).await
    }

    async fn cancel_image_model_download(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        dispatch(ctx, "systemCancelImageModelDownload", false).await
    }

    /// `force` re-scans images that already have an embedding.
    async fn start_image_index(
        &self,
        ctx: &Context<'_>,
        force: Option<bool>,
    ) -> async_graphql::Result<bool> {
        dispatch(ctx, "systemStartImageIndex", force.unwrap_or(false)).await
    }

    async fn cancel_image_index(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        dispatch(ctx, "systemCancelImageIndex", false).await
    }
}

/// Only `startImageIndex` carries a `force`; the other commands send it as
/// false so the host sees one payload shape for the whole group.
async fn dispatch(ctx: &Context<'_>, method: &str, force: bool) -> async_graphql::Result<bool> {
    ctx.data_unchecked::<Arc<Host>>()
        .call(method, serde_json::json!({ "force": force }))
        .await
        .map_err(|error| async_graphql::Error::new(error))?;
    Ok(true)
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/public_image_index.rs"]
mod tests;
