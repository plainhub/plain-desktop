use crate::{
    content_types::{Instant, Long},
    db::{Db, MediaItemRow},
    enums::DataType,
    library::{image_embeddings as embeddings, media_metadata as metadata},
};
use async_graphql::{Context, ID, InputObject, Object, Result, SimpleObject};
use std::sync::Arc;
#[derive(SimpleObject)]
struct MediaHostDuration {
    r#type: DataType,
    media_id: ID,
    duration_ms: Long,
    updated_at: Instant,
}
fn duration(row: MediaItemRow) -> Result<MediaHostDuration> {
    let kind = match row.media_type.as_str() {
        "audio" => DataType::Audio,
        "video" => DataType::Video,
        _ => return Err("invalid stored media type".into()),
    };
    Ok(MediaHostDuration {
        r#type: kind,
        media_id: row.media_id.into(),
        duration_ms: Long(row.duration_ms),
        updated_at: super::content_common::instant(&row.updated_at)?,
    })
}
#[derive(InputObject)]
struct ImageHostEmbeddingInput {
    id: ID,
    path: String,
    embedding_base64: String,
}
impl From<ImageHostEmbeddingInput> for embeddings::EmbeddingInput {
    fn from(i: ImageHostEmbeddingInput) -> Self {
        Self {
            id: i.id.to_string(),
            path: i.path,
            embedding_base64: i.embedding_base64,
        }
    }
}
#[derive(SimpleObject)]
struct ImageHostSearchResult {
    image_id: ID,
    score: f32,
}
#[derive(Default)]
pub struct MediaAuxQuery;
#[Object]
impl MediaAuxQuery {
    async fn media_host_durations(&self, ctx: &Context<'_>) -> Result<Vec<MediaHostDuration>> {
        metadata::all(ctx.data::<Arc<Db>>()?)?
            .into_iter()
            .map(duration)
            .collect()
    }
    async fn image_host_embedding_ids(&self, ctx: &Context<'_>) -> Result<Vec<ID>> {
        Ok(embeddings::ids(ctx.data::<Arc<Db>>()?)?
            .into_iter()
            .map(ID)
            .collect())
    }
    async fn image_host_embedding_count(&self, ctx: &Context<'_>) -> Result<Long> {
        Ok(Long(embeddings::count(ctx.data::<Arc<Db>>()?)?))
    }
    async fn image_host_search(
        &self,
        ctx: &Context<'_>,
        embedding_base64: String,
        max_results: i32,
    ) -> Result<Vec<ImageHostSearchResult>> {
        let db = ctx.data::<Arc<Db>>()?.clone();
        let max_results = usize::try_from(max_results)?;
        let rows = tokio::task::spawn_blocking(move || {
            embeddings::search(&db, &embedding_base64, max_results)
        })
        .await??;
        Ok(rows
            .into_iter()
            .map(|r| ImageHostSearchResult {
                image_id: r.image_id.into(),
                score: r.score,
            })
            .collect())
    }
}
#[derive(Default)]
pub struct MediaAuxMutation;
#[Object]
impl MediaAuxMutation {
    async fn media_host_save_duration(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        media_id: ID,
        duration_ms: Long,
    ) -> Result<bool> {
        metadata::save(
            ctx.data::<Arc<Db>>()?,
            r#type.media_type_str().ok_or("invalid media type")?,
            media_id.as_str(),
            duration_ms.0,
        )?;
        Ok(true)
    }
    async fn media_host_remove_durations(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        media_ids: Vec<ID>,
    ) -> Result<Long> {
        Ok(Long(
            metadata::delete(
                ctx.data::<Arc<Db>>()?,
                r#type.media_type_str().ok_or("invalid media type")?,
                &media_ids
                    .into_iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>(),
            )?
            .try_into()?,
        ))
    }
    async fn image_host_save_embeddings(
        &self,
        ctx: &Context<'_>,
        items: Vec<ImageHostEmbeddingInput>,
    ) -> Result<bool> {
        embeddings::save(
            ctx.data::<Arc<Db>>()?,
            &items.into_iter().map(Into::into).collect::<Vec<_>>(),
        )?;
        Ok(true)
    }
    async fn image_host_remove_embeddings(&self, ctx: &Context<'_>, ids: Vec<ID>) -> Result<Long> {
        Ok(Long(
            ctx.data::<Arc<crate::content_api::image_index::ImageIndex>>()?
                .remove(&ids.into_iter().map(|id| id.to_string()).collect::<Vec<_>>())?
                .try_into()?,
        ))
    }
    async fn image_host_clear_embeddings(&self, ctx: &Context<'_>) -> Result<Long> {
        Ok(Long(
            ctx.data::<Arc<crate::content_api::image_index::ImageIndex>>()?
                .clear()?
                .try_into()?,
        ))
    }
}
