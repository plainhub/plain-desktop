//! `tags` / `tagRelations` queries and the tag mutations — backed by the
//! shared plain-rs library core (`local_library.db`), the same behavior
//! the NAS server runs.
//!
//! The `addToTags` / `removeFromTags` mutations take the plain-app search
//! DSL `query`. The desktop local server has no media index, so only the
//! explicit `ids:a,b,c` checkbox form resolves; page-DSL queries match
//! nothing and are a documented no-op (`true`).

use async_graphql::{Context, ID, Object};
use std::sync::Arc;

use crate::local::db::tag_store;
use crate::local::enums::DataType;
use crate::local::graphql::context::AppCtx;
use crate::local::graphql::schema::types::{Tag, TagRelation, TagRelationStub};

/// Numeric plain-app `DataType` ordinal — the tag's stored `type` column.
fn kind_of(t: DataType) -> i32 {
    match t {
        DataType::Default => 0,
        DataType::Audio => 1,
        DataType::Video => 2,
        DataType::Image => 3,
        DataType::Sms => 4,
        DataType::Contact => 5,
        DataType::Note => 6,
        DataType::FeedEntry => 7,
        DataType::Call => 8,
        DataType::Package => 9,
        DataType::File => 10,
        DataType::AppFile => 11,
        DataType::Doc => 12,
    }
}

fn tag_to_gql(t: plain_rs::library::db::TagRow) -> Tag {
    Tag {
        id: t.id,
        name: t.name,
        count: t.count,
    }
}

/// Extract the explicit `ids:a,b,c` selection from a mutation query —
/// same shared DSL parser (plain-rs `utils::search_dsl`) the NAS server
/// uses. Returns `None` for page-DSL queries (unresolvable without a
/// media index).
fn parse_ids_query(query: &str) -> Option<Vec<String>> {
    let ids = plain_rs::utils::search_dsl::field_value(query, "ids")?;
    if ids.trim().is_empty() {
        return None;
    }
    Some(
        ids.split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

#[derive(Default)]
pub struct TagQuery;

#[Object]
impl TagQuery {
    /// List tags for the given data type, live counts.
    async fn tags(&self, ctx: &Context<'_>, r#type: DataType) -> Vec<Tag> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        tag_store::tags_by_type(&c.library, kind_of(r#type))
            .into_iter()
            .map(tag_to_gql)
            .collect()
    }

    /// Look up tag relations for entity keys, filtered to the data type.
    async fn tag_relations(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        keys: Vec<String>,
    ) -> Vec<TagRelation> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        tag_store::relations_for_keys_of_kind(&c.library, &keys, kind_of(r#type))
            .into_iter()
            .map(|r| TagRelation {
                tag_id: r.tag_id,
                key: r.key,
            })
            .collect()
    }
}

#[derive(Default)]
pub struct TagMutation;

#[Object]
impl TagMutation {
    /// Create a tag for the given data type and return it.
    async fn create_tag(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        name: String,
    ) -> async_graphql::Result<Tag> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let t = tag_store::create_tag(&c.library, kind_of(r#type), &name)
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(tag_to_gql(t))
    }

    /// Rename a tag and return the updated entity (missing ids error —
    /// the phone contract returns `Tag!`).
    async fn update_tag(
        &self,
        ctx: &Context<'_>,
        id: ID,
        name: String,
    ) -> async_graphql::Result<Tag> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        tag_store::update_tag(&c.library, id.as_ref(), &name)
            .map_err(|e| async_graphql::Error::new(e.to_string()))?
            .map(tag_to_gql)
            .ok_or_else(|| async_graphql::Error::new(format!("tag not found: {}", id.0)))
    }

    /// Delete a tag together with its relations.
    async fn delete_tag(&self, ctx: &Context<'_>, id: ID) -> bool {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        tag_store::delete_tag(&c.library, id.as_ref());
        true
    }

    /// Add the items matching `query` (explicit `ids:` form only in local
    /// mode) to multiple tags. No duplicate relations, same as plain-app.
    async fn add_to_tags(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        tag_ids: Vec<ID>,
        query: String,
    ) -> bool {
        let _ = r#type;
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let tag_ids: Vec<String> = tag_ids
            .iter()
            .map(|t| t.to_string().trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        let Some(keys) = parse_ids_query(&query) else {
            return true;
        };
        let mut rels = Vec::new();
        for tid in &tag_ids {
            for k in &keys {
                rels.push((tid.clone(), k.clone()));
            }
        }
        tag_store::add_relations(&c.library, &rels);
        true
    }

    /// Per-item tag relation edit: adds and/or removes tag associations.
    async fn update_tag_relations(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        item: TagRelationStub,
        add_tag_ids: Vec<ID>,
        remove_tag_ids: Vec<ID>,
    ) -> bool {
        let _ = r#type;
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let add: Vec<(String, String)> = add_tag_ids
            .iter()
            .filter(|t| !t.is_empty())
            .map(|t| (t.to_string(), item.key.clone()))
            .collect();
        if !add.is_empty() {
            tag_store::add_relations(&c.library, &add);
        }
        if !remove_tag_ids.is_empty() {
            let remove: Vec<String> = remove_tag_ids.iter().map(|i| i.to_string()).collect();
            tag_store::remove_relations(&c.library, std::slice::from_ref(&item.key), &remove);
        }
        true
    }

    /// Remove the items matching `query` from the given tags — the inverse
    /// of `addToTags` (`ids:` form only in local mode).
    async fn remove_from_tags(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        tag_ids: Vec<ID>,
        query: String,
    ) -> bool {
        let _ = r#type;
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let tag_ids: Vec<String> = tag_ids
            .iter()
            .map(|t| t.to_string().trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        let Some(keys) = parse_ids_query(&query) else {
            return true;
        };
        tag_store::remove_relations(&c.library, &keys, &tag_ids);
        true
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/local/graphql/schema/tag.rs"]
mod tests;
