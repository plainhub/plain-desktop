use crate::graphql::resolvers::images::image_editor_project::ImageEditorProjectSummary;
use crate::graphql::resolvers::images::image_editor_project::to_summary;
use async_graphql::{Context, Object};
use plain_rs::db::Db;
use plain_rs::db::image_editor_project as store;
use std::sync::Arc;
#[derive(Default)]
pub struct ImageEditorItemsQuery;
#[Object]
impl ImageEditorItemsQuery {
    async fn image_editor_project_items(
        &self,
        ctx: &Context<'_>,
        query: String,
        offset: i32,
        limit: i32,
    ) -> async_graphql::Result<Vec<ImageEditorProjectSummary>> {
        if offset < 0 || limit < 0 {
            return Err(async_graphql::Error::new("invalid pagination"));
        }
        store::summaries(ctx.data::<Arc<Db>>()?, offset, limit, &query)?
            .into_iter()
            .map(to_summary)
            .collect()
    }
}
