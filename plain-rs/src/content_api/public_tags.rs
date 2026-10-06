//! Public `/graphql` tag roots.
//!
//! Tags and their relations are Rust SQLite rows, so nothing here needs the
//! platform — except resolving a query DSL into the keys a relation attaches
//! to, which for the media kinds is a media-store lookup.
//!
//! The contract's `Tag` has no numeric kind field (the list it came from
//! implies it), while [`crate::content_types::Tag`] carries one for the
//! desktop. Both want the GraphQL name `Tag` and async-graphql refuses the
//! duplicate, so this module uses [`super::public_contact_types::Tag`].

use super::host::Host;
use super::public_contact_types::Tag;
use crate::content_types::{Long, TagRelation};
use crate::db::{Db, TagRow};
use crate::library::{tag_records, tags};
use async_graphql::{Context, Enum, InputObject, Object};
use serde_json::{Value, json};
use std::sync::Arc;

/// The contract's content kinds. The numeric values are plain-app's
/// `DataType` ordinals and are what the tag tables store, so they are the
/// contract's order rather than a free choice.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum DataType {
    Default,
    Audio,
    Video,
    Image,
    Sms,
    Contact,
    Note,
    FeedEntry,
    Call,
    Package,
    File,
    AppFile,
    Doc,
}

impl DataType {
    fn kind(self) -> i32 {
        match self {
            DataType::Default => 0,
            DataType::Audio => 1,
            DataType::Video => 2,
            DataType::Image => 3,
            DataType::Sms => 4,
            DataType::Contact => 5,
            DataType::Note => 6,
            DataType::FeedEntry => 7,
            DataType::Call => 8,
            DataType::Package => 21,
            DataType::File => 22,
            DataType::AppFile => 23,
            DataType::Doc => 24,
        }
    }

    /// The kinds a query DSL can be resolved against. NOTE and FEED_ENTRY
    /// are absent on purpose: their id readers live behind the notes and
    /// feeds roots, and a tag write must not re-enter the public schema to
    /// reach them.
    fn tags_queryable(self) -> bool {
        matches!(
            self,
            DataType::Audio
                | DataType::Video
                | DataType::Image
                | DataType::Doc
                | DataType::Call
                | DataType::Contact
                | DataType::Sms
        )
    }
}

#[derive(InputObject, Clone, Debug)]
pub struct TagRelationStub {
    pub key: String,
    pub title: String,
    pub size: Long,
}

#[derive(Default)]
pub struct TagsQuery;

#[Object]
impl TagsQuery {
    async fn tags(&self, ctx: &Context<'_>, r#type: DataType) -> async_graphql::Result<Vec<Tag>> {
        let db = db(ctx);
        Ok(tags::tags_by_type(db, r#type.kind())
            .map_err(failed)?
            .iter()
            .map(tag)
            .collect())
    }

    async fn tag_relations(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        keys: Vec<String>,
    ) -> async_graphql::Result<Vec<TagRelation>> {
        let db = db(ctx);
        Ok(tags::relations_for_keys_of_kind(db, &keys, r#type.kind())
            .map_err(failed)?
            .iter()
            .map(|row| TagRelation {
                tag_id: async_graphql::ID::from(row.tag_id.clone()),
                key: row.key.clone(),
            })
            .collect())
    }

    /// `null` when the id is unknown — the web client treats that as "gone"
    /// and drops the tag from its list.
    async fn tag(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<Option<Tag>> {
        let db = db(ctx);
        Ok(tags::tag_by_id(db, id.as_str())
            .map_err(failed)?
            .as_ref()
            .map(tag))
    }

    async fn tag_keys(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<Vec<String>> {
        let db = db(ctx);
        tags::keys_for_tag(db, id.as_str()).map_err(failed)
    }
}

#[derive(Default)]
pub struct TagsMutation;

#[Object]
impl TagsMutation {
    async fn create_tag(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        name: String,
    ) -> async_graphql::Result<Tag> {
        let row = tags::create_tag(db(ctx), r#type.kind(), &name).map_err(failed)?;
        Ok(tag(&row))
    }

    async fn update_tag(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        name: String,
    ) -> async_graphql::Result<Tag> {
        let row = tags::update_tag(db(ctx), id.as_str(), &name).map_err(failed)?;
        row.as_ref()
            .map(tag)
            .ok_or_else(|| async_graphql::Error::new(format!("Tag {} not found", id.as_str())))
    }

    async fn delete_tag(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        tags::delete_tag(db(ctx), id.as_str()).map_err(failed)?;
        Ok(true)
    }

    /// Attaches every tag to every key the query matches. An empty query
    /// would tag the whole library, so it is rejected before the platform
    /// ever sees it.
    async fn add_to_tags(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        tag_ids: Vec<async_graphql::ID>,
        query: String,
    ) -> async_graphql::Result<bool> {
        let stubs = tag_query_stubs(ctx, r#type, &query).await?;
        // `add_relations` takes (tag_id, key) pairs, in that order.
        let relations: Vec<(String, String)> = stubs
            .iter()
            .flat_map(|stub| {
                tag_ids
                    .iter()
                    .map(|tag_id| (tag_id.to_string(), stub.key.clone()))
            })
            .collect();
        tags::add_relations(db(ctx), &relations).map_err(failed)?;
        Ok(true)
    }

    /// Same key resolution as `addToTags`, minus the title and size the
    /// relation stub carries — removal has nothing to record.
    async fn remove_from_tags(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        tag_ids: Vec<async_graphql::ID>,
        query: String,
    ) -> async_graphql::Result<bool> {
        let keys = tag_query_keys(ctx, r#type, &query).await?;
        let ids: Vec<String> = tag_ids.iter().map(|tag_id| tag_id.to_string()).collect();
        tags::remove_relations(db(ctx), &keys, &ids).map_err(failed)?;
        Ok(true)
    }

    /// One key, an explicit add and remove list. The stub's title and size are
    /// stored on the relation: the tag editor shows them without a second
    /// lookup, so dropping them would make a tagged row render blank.
    async fn update_tag_relations(
        &self,
        ctx: &Context<'_>,
        r#type: DataType,
        item: TagRelationStub,
        add_tag_ids: Vec<async_graphql::ID>,
        remove_tag_ids: Vec<async_graphql::ID>,
    ) -> async_graphql::Result<bool> {
        let add: Vec<String> = add_tag_ids
            .iter()
            .map(|tag_id| tag_id.to_string())
            .collect();
        let remove: Vec<String> = remove_tag_ids
            .iter()
            .map(|tag_id| tag_id.to_string())
            .collect();
        tag_records::edit(
            db(ctx),
            r#type.kind(),
            &item.key,
            &item.title,
            item.size.0,
            &add,
            &remove,
        )
        .map_err(failed)?;
        Ok(true)
    }
}

/// Resolves the query into the keys relations should attach to, with the
/// title and size the tag editor shows on each row.
async fn tag_query_stubs(
    ctx: &Context<'_>,
    r#type: DataType,
    query: &str,
) -> async_graphql::Result<Vec<TagRelationStub>> {
    require_queryable(r#type, query)?;
    let facts = host_call(
        ctx,
        "systemTagQueryStubs",
        json!({ "dataType": r#type.name(), "query": query }),
    )
    .await?;
    Ok(super::public_facts::rows(&facts, |item| TagRelationStub {
        key: super::public_facts::text(item, "key"),
        title: super::public_facts::text(item, "title"),
        size: Long(super::public_facts::integer(item, "size")),
    }))
}

/// Resolves the query into bare keys, for the removal path.
async fn tag_query_keys(
    ctx: &Context<'_>,
    r#type: DataType,
    query: &str,
) -> async_graphql::Result<Vec<String>> {
    require_queryable(r#type, query)?;
    let facts = host_call(
        ctx,
        "systemTagQueryKeys",
        json!({ "dataType": r#type.name(), "query": query }),
    )
    .await?;
    Ok(super::public_facts::strings(&facts, "ids"))
}

/// A blank query would match every row of the kind. That is never what a
/// client meant, and the desktop UI has no way to express "tag everything",
/// so it is refused rather than silently applied.
fn require_queryable(r#type: DataType, query: &str) -> async_graphql::Result<()> {
    if !r#type.tags_queryable() {
        return Err(async_graphql::Error::new(format!(
            "Unsupported tag query type: {}",
            r#type.name()
        )));
    }
    if query.trim().is_empty() {
        return Err(async_graphql::Error::new("explicit query required"));
    }
    Ok(())
}

impl DataType {
    /// The contract's spelling, which is also what the platform's own
    /// `DataType` enum uses.
    fn name(self) -> &'static str {
        match self {
            DataType::Default => "DEFAULT",
            DataType::Audio => "AUDIO",
            DataType::Video => "VIDEO",
            DataType::Image => "IMAGE",
            DataType::Sms => "SMS",
            DataType::Contact => "CONTACT",
            DataType::Note => "NOTE",
            DataType::FeedEntry => "FEED_ENTRY",
            DataType::Call => "CALL",
            DataType::Package => "PACKAGE",
            DataType::File => "FILE",
            DataType::AppFile => "APP_FILE",
            DataType::Doc => "DOC",
        }
    }
}

fn tag(row: &TagRow) -> Tag {
    Tag {
        id: async_graphql::ID::from(row.id.clone()),
        name: row.name.clone(),
        count: row.count,
    }
}

fn failed(error: crate::library::LibraryError) -> async_graphql::Error {
    async_graphql::Error::new(error.to_string())
}

fn db<'a>(ctx: &'a Context<'_>) -> &'a Arc<Db> {
    ctx.data_unchecked::<Arc<Db>>()
}

async fn host_call(ctx: &Context<'_>, method: &str, params: Value) -> async_graphql::Result<Value> {
    ctx.data_unchecked::<Arc<Host>>()
        .call(method, params)
        .await
        .map_err(async_graphql::Error::new)
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/public_tags.rs"]
mod tests;
