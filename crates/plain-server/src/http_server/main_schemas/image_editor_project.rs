use super::types::Instant;
use async_graphql::{Context, ID, InputObject, Object, SimpleObject};
use std::sync::Arc;

use crate::db::{Db, image_editor_project as store};
use crate::image_editor::Updates;

#[derive(SimpleObject)]
pub struct ImageEditorProject {
    pub id: ID,
    pub state_b64: String,
    pub thumbnail: Option<String>,
    pub canvas_width: i32,
    pub canvas_height: i32,
    pub layer_count: i32,
    pub created_at: Instant,
    pub updated_at: Instant,
}

#[derive(SimpleObject)]
pub struct ImageEditorProjectSummary {
    pub id: ID,
    pub thumbnail: Option<String>,
    pub canvas_width: i32,
    pub canvas_height: i32,
    pub layer_count: i32,
    pub updated_at: Instant,
}

#[derive(InputObject)]
pub struct ImageEditorProjectInput {
    pub state_b64: String,
    pub thumbnail: Option<String>,
    pub canvas_width: i32,
    pub canvas_height: i32,
    pub layer_count: i32,
}

fn instant(value: &str) -> async_graphql::Result<Instant> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|value| Instant(value.with_timezone(&chrono::Utc)))
        .map_err(|error| async_graphql::Error::new(format!("invalid stored timestamp: {error}")))
}

fn to_project(row: store::ImageEditorProjectRow) -> async_graphql::Result<ImageEditorProject> {
    Ok(ImageEditorProject {
        id: ID(row.id),
        state_b64: row.state_b64,
        thumbnail: row.thumbnail,
        canvas_width: row.canvas_width,
        canvas_height: row.canvas_height,
        layer_count: row.layer_count,
        created_at: instant(&row.created_at)?,
        updated_at: instant(&row.updated_at)?,
    })
}

pub(super) fn to_summary(
    row: store::ImageEditorProjectRow,
) -> async_graphql::Result<ImageEditorProjectSummary> {
    Ok(ImageEditorProjectSummary {
        id: ID(row.id),
        thumbnail: row.thumbnail,
        canvas_width: row.canvas_width,
        canvas_height: row.canvas_height,
        layer_count: row.layer_count,
        updated_at: instant(&row.updated_at)?,
    })
}

#[derive(Default)]
pub struct ImageEditorProjectQuery;

#[Object]
impl ImageEditorProjectQuery {
    async fn image_editor_projects(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<ImageEditorProjectSummary>> {
        store::summaries(ctx.data::<Arc<Db>>()?, 0, 20, "")?
            .into_iter()
            .map(to_summary)
            .collect()
    }

    async fn image_editor_project(
        &self,
        ctx: &Context<'_>,
        id: ID,
    ) -> async_graphql::Result<Option<ImageEditorProject>> {
        store::get(ctx.data::<Arc<Db>>()?, id.as_str())?
            .map(to_project)
            .transpose()
    }
}

#[derive(Default)]
pub struct ImageEditorProjectMutation;

#[Object]
impl ImageEditorProjectMutation {
    async fn save_image_editor_project(
        &self,
        ctx: &Context<'_>,
        id: ID,
        input: ImageEditorProjectInput,
    ) -> async_graphql::Result<ImageEditorProject> {
        if input.canvas_width < 0
            || input.canvas_height < 0
            || input.layer_count < 0
            || input.state_b64.len() > crate::image_editor::MAX_STATE_BYTES
            || input
                .thumbnail
                .as_ref()
                .is_some_and(|s| s.len() > 1024 * 1024)
        {
            return Err(async_graphql::Error::new("invalid image editor project"));
        }
        crate::image_editor::decode(&input.state_b64).map_err(async_graphql::Error::new)?;
        let id = if id.is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            id.to_string()
        };
        let row = store::save(
            ctx.data::<Arc<Db>>()?,
            &id,
            &input.state_b64,
            input.thumbnail.as_deref(),
            input.canvas_width,
            input.canvas_height,
            input.layer_count,
        )?;
        to_project(row)
    }

    async fn delete_image_editor_project(
        &self,
        ctx: &Context<'_>,
        id: ID,
    ) -> async_graphql::Result<bool> {
        store::delete(ctx.data::<Arc<Db>>()?, id.as_str())?;
        Ok(true)
    }

    async fn broadcast_image_editor_update(
        &self,
        ctx: &Context<'_>,
        id: ID,
        update: String,
    ) -> async_graphql::Result<bool> {
        ctx.data::<Arc<Updates>>()?
            .publish(id.as_str(), &update)
            .map_err(async_graphql::Error::new)?;
        Ok(true)
    }
}
