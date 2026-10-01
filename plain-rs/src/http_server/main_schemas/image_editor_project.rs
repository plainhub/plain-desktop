use super::types::Instant;
use async_graphql::{Context, ID, InputObject, Object, SimpleObject};
use std::sync::Arc;

use crate::api::context::{AppCtx, WS_IMAGE_EDITOR_UPDATE, WsEvent};
use crate::db::{Db, image_editor_project as store};

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

fn to_summary(
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
        store::list(ctx.data::<Arc<Db>>()?, 20)?
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
        let id_bytes = id.as_str().as_bytes();
        if id_bytes.len() > u8::MAX as usize {
            return Err(async_graphql::Error::new("invalid_image_editor_project_id"));
        }
        let update_bytes = crate::utils::base64::base64_decode(&update);
        let mut payload = Vec::with_capacity(1 + id_bytes.len() + update_bytes.len());
        payload.push(id_bytes.len() as u8);
        payload.extend_from_slice(id_bytes);
        payload.extend_from_slice(&update_bytes);
        let app = ctx.data_unchecked::<Arc<AppCtx>>();
        let _ = app
            .event_tx
            .send(WsEvent::broadcast_binary(WS_IMAGE_EDITOR_UPDATE, payload));
        Ok(true)
    }
}
