use super::image_editor_project::{ImageEditorProjectSummary, to_summary};
use crate::db::{Db, image_editor_project as store};
use async_graphql::{Context, Object};
use std::sync::Arc;
#[derive(Default)]
pub struct ImageEditorItemsQuery;
#[Object]
impl ImageEditorItemsQuery {
    async fn image_editor_project_items(
        &self,
        ctx: &Context<'_>,
        offset: i32,
        limit: i32,
        query: String,
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
