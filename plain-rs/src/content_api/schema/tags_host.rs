use crate::{
    content_types::{Instant, Long},
    db::Db,
    enums::DataType,
    library::tag_records as domain,
};
use async_graphql::{Context, ID, InputObject, Object, Result, SimpleObject};
use std::sync::Arc;
#[derive(SimpleObject)]
struct TagHostRecord {
    id: ID,
    name: String,
    r#type: i32,
    count: i32,
    created_at: Instant,
    updated_at: Instant,
}
fn tag(r: domain::TagRecord) -> Result<TagHostRecord> {
    Ok(TagHostRecord {
        id: r.id.into(),
        name: r.name,
        r#type: r.kind,
        count: r.count,
        created_at: super::content_common::instant(&r.created_at)?,
        updated_at: super::content_common::instant(&r.updated_at)?,
    })
}
#[derive(SimpleObject)]
struct TagHostRelation {
    tag_id: ID,
    key: String,
    r#type: i32,
    created_at: Instant,
    size_bytes: Long,
    title: String,
}
fn relation(r: domain::RelationRecord) -> Result<TagHostRelation> {
    Ok(TagHostRelation {
        tag_id: r.tag_id.into(),
        key: r.key,
        r#type: r.kind,
        created_at: super::content_common::instant(&r.created_at)?,
        size_bytes: Long(r.size_bytes),
        title: r.title,
    })
}
#[derive(InputObject)]
struct TagHostRelationInput {
    tag_id: ID,
    key: String,
    r#type: DataType,
    size_bytes: Long,
    title: String,
}
impl From<TagHostRelationInput> for domain::RelationInput {
    fn from(r: TagHostRelationInput) -> Self {
        Self {
            tag_id: r.tag_id.to_string(),
            key: r.key,
            kind: r.r#type.kind(),
            size_bytes: r.size_bytes.0,
            title: r.title,
        }
    }
}
#[derive(InputObject)]
struct TagHostItemInput {
    key: String,
    size_bytes: Long,
    title: String,
}
fn ids(items: Vec<ID>) -> Vec<String> {
    items.into_iter().map(|id| id.to_string()).collect()
}
#[derive(Default)]
pub struct TagHostQuery;
#[Object]
impl TagHostQuery {
    async fn tag_host_all(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
    ) -> Result<Vec<TagHostRecord>> {
        domain::all(ctx.data::<Arc<Db>>()?, r#type.kind())?
            .into_iter()
            .map(tag)
            .collect()
    }
    async fn tag_host_get(&self, ctx: &Context<'_>, id: ID) -> Result<Option<TagHostRecord>> {
        domain::get(ctx.data::<Arc<Db>>()?, id.as_str())?
            .map(tag)
            .transpose()
    }
    async fn tag_host_relations(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        keys: Vec<String>,
    ) -> Result<Vec<TagHostRelation>> {
        domain::relations(ctx.data::<Arc<Db>>()?, r#type.kind(), &keys)?
            .into_iter()
            .map(relation)
            .collect()
    }
    async fn tag_host_intersection(
        &self,
        ctx: &Context<'_>,
        tag_ids: Vec<ID>,
    ) -> Result<Vec<String>> {
        Ok(domain::intersection(ctx.data::<Arc<Db>>()?, &ids(tag_ids))?)
    }
}
#[derive(Default)]
pub struct TagHostMutation;
#[Object]
impl TagHostMutation {
    async fn tag_host_add(
        &self,
        ctx: &Context<'_>,
        items: Vec<TagHostRelationInput>,
    ) -> Result<bool> {
        domain::add(
            ctx.data::<Arc<Db>>()?,
            &items.into_iter().map(Into::into).collect::<Vec<_>>(),
        )?;
        Ok(true)
    }
    async fn tag_host_edit(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        item: TagHostItemInput,
        add_tag_ids: Vec<ID>,
        remove_tag_ids: Vec<ID>,
    ) -> Result<bool> {
        domain::edit(
            ctx.data::<Arc<Db>>()?,
            r#type.kind(),
            &item.key,
            &item.title,
            item.size_bytes.0,
            &ids(add_tag_ids),
            &ids(remove_tag_ids),
        )?;
        Ok(true)
    }
    async fn tag_host_remove(
        &self,
        ctx: &Context<'_>,
        keys: Vec<String>,
        tag_ids: Vec<ID>,
    ) -> Result<bool> {
        domain::remove(ctx.data::<Arc<Db>>()?, &keys, &ids(tag_ids))?;
        Ok(true)
    }
    async fn tag_host_remove_keys(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        keys: Vec<String>,
    ) -> Result<bool> {
        domain::remove_keys(ctx.data::<Arc<Db>>()?, r#type.kind(), &keys)?;
        Ok(true)
    }
    async fn tag_host_clear_type(&self, ctx: &Context<'_>, r#type: DataType) -> Result<bool> {
        domain::clear_type(ctx.data::<Arc<Db>>()?, r#type.kind())?;
        Ok(true)
    }
    async fn tag_host_clear_tag(&self, ctx: &Context<'_>, id: ID) -> Result<bool> {
        domain::clear_tag(ctx.data::<Arc<Db>>()?, id.as_str())?;
        Ok(true)
    }
}
