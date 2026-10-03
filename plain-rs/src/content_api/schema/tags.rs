use crate::content_types::{Tag, TagRelation, TagRelationStub};
use crate::{
    db::{Db, TagRow},
    enums::DataType,
    library::tags,
};
use async_graphql::{Context, ID, Object, Result};
use std::sync::Arc;
fn model(t: TagRow) -> Tag {
    Tag {
        id: ID(t.id),
        name: t.name,
        r#type: t.kind,
        count: t.count,
    }
}
fn keys(db: &Db, kind: DataType, query: &str) -> Result<Vec<String>> {
    if query.trim().is_empty() {
        return Err("query is required".into());
    }
    Ok(match kind {
        DataType::Note => db.note_ids(query, None)?,
        DataType::FeedEntry => db
            .feed_entries_list(query, i64::MAX, 0)?
            .into_iter()
            .map(|e| e.id)
            .collect(),
        _ => return Err("unsupported content type".into()),
    })
}
#[derive(Default)]
pub struct TagQuery;
#[Object]
impl TagQuery {
    async fn tags(&self, ctx: &Context<'_>, r#type: DataType) -> Result<Vec<Tag>> {
        let db = ctx.data::<Arc<Db>>()?;
        Ok(tags::tags_by_type(db, r#type.kind())?
            .into_iter()
            .map(model)
            .collect())
    }
    async fn tag(&self, ctx: &Context<'_>, id: ID) -> Result<Option<Tag>> {
        Ok(tags::tag_by_id(ctx.data::<Arc<Db>>()?, id.as_str())?.map(model))
    }
    async fn tag_keys(&self, ctx: &Context<'_>, id: ID) -> Result<Vec<String>> {
        Ok(tags::keys_for_tag(ctx.data::<Arc<Db>>()?, id.as_str())?)
    }
    async fn tag_relations(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        keys: Vec<String>,
    ) -> Result<Vec<TagRelation>> {
        Ok(
            tags::relations_for_keys_of_kind(ctx.data::<Arc<Db>>()?, &keys, r#type.kind())?
                .into_iter()
                .map(|r| TagRelation {
                    tag_id: ID(r.tag_id),
                    key: r.key,
                })
                .collect(),
        )
    }
}
#[derive(Default)]
pub struct TagMutation;
#[Object]
impl TagMutation {
    async fn create_tag(&self, ctx: &Context<'_>, r#type: DataType, name: String) -> Result<Tag> {
        Ok(model(tags::create_tag(
            ctx.data::<Arc<Db>>()?,
            r#type.kind(),
            &name,
        )?))
    }
    async fn update_tag(&self, ctx: &Context<'_>, id: ID, name: String) -> Result<Tag> {
        Ok(model(
            tags::update_tag(ctx.data::<Arc<Db>>()?, id.as_str(), &name)?
                .ok_or_else(|| async_graphql::Error::new("tag not found"))?,
        ))
    }
    async fn delete_tag(&self, ctx: &Context<'_>, id: ID) -> Result<bool> {
        tags::delete_tag(ctx.data::<Arc<Db>>()?, id.as_str())?;
        Ok(true)
    }
    async fn add_to_tags(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        tag_ids: Vec<ID>,
        query: String,
    ) -> Result<bool> {
        let db = ctx.data::<Arc<Db>>()?;
        let keys = keys(db, r#type, &query)?;
        for tag in &tag_ids {
            if tags::tag_by_id(db, tag.as_str())?.is_none_or(|t| t.kind != r#type.kind()) {
                return Err("tag type mismatch".into());
            }
        }
        tags::add_relations(
            db,
            &tag_ids
                .iter()
                .flat_map(|id| keys.iter().map(move |k| (id.to_string(), k.clone())))
                .collect::<Vec<_>>(),
        )?;
        Ok(true)
    }
    async fn update_tag_relations(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        item: TagRelationStub,
        add_tag_ids: Vec<ID>,
        remove_tag_ids: Vec<ID>,
    ) -> Result<bool> {
        crate::library::tag_records::edit(
            ctx.data::<Arc<Db>>()?,
            r#type.kind(),
            &item.key,
            &item.title,
            item.size.0,
            &add_tag_ids
                .into_iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>(),
            &remove_tag_ids
                .into_iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>(),
        )?;
        Ok(true)
    }
    async fn remove_from_tags(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        tag_ids: Vec<ID>,
        query: String,
    ) -> Result<bool> {
        let db = ctx.data::<Arc<Db>>()?;
        tags::remove_relations(
            db,
            &keys(db, r#type, &query)?,
            &tag_ids
                .into_iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>(),
        )?;
        Ok(true)
    }
}
