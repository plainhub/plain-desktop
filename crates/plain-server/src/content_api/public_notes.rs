//! Public `/graphql` note roots.
//!
//! Notes are Rust SQLite rows, so the whole surface is local. The app's own
//! note roots could not be mounted here because their `tags` field is the
//! desktop `Tag`, which carries a numeric kind the contract does not have —
//! and two types claiming `Tag` in one registry is a build-time failure, not
//! a merge conflict.

use super::public_contact_types::Tag;
use super::public_facts::stored_instant;
use crate::content_types::{ActionResult, Instant};
use crate::db::{Db, notes_feeds::NoteRow};
use crate::enums::DataType;
use crate::library::tags;
use crate::notes;
use async_graphql::{Context, InputObject, Object, SimpleObject};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(SimpleObject, Clone, Debug)]
pub struct Note {
    pub id: async_graphql::ID,
    pub title: String,
    pub content: String,
    #[graphql(name = "deletedAt")]
    pub deleted_at: Option<Instant>,
    #[graphql(name = "createdAt")]
    pub created_at: Instant,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
    pub tags: Vec<Tag>,
}

#[derive(InputObject, Clone, Debug)]
pub struct NoteInput {
    pub title: String,
    pub content: String,
}

#[derive(Default)]
pub struct NotesQuery;

#[Object]
impl NotesQuery {
    async fn note_count(&self, ctx: &Context<'_>, query: String) -> async_graphql::Result<i32> {
        Ok(notes::count(db(ctx)?, &query)?)
    }

    async fn note(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<Option<Note>> {
        let library = db(ctx)?;
        let Some(row) = notes::get(library, id.as_str())? else {
            return Ok(None);
        };
        let mut tags = tags_by_key(library, &[row.id.clone()], DataType::Note)?;
        let tagged = tags.remove(&row.id).unwrap_or_default();
        Ok(Some(note(row, tagged)))
    }

    async fn notes(
        &self,
        ctx: &Context<'_>,
        offset: i32,
        limit: i32,
        query: String,
    ) -> async_graphql::Result<Vec<Note>> {
        let library = db(ctx)?;
        let rows = notes::search(library, &query, limit, offset)?;
        let tags = tags_by_key(
            library,
            &rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>(),
            DataType::Note,
        )?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let tagged = tags.get(&row.id).cloned().unwrap_or_default();
                note(row, tagged)
            })
            .collect())
    }
}

#[derive(Default)]
pub struct NotesMutation;

#[Object]
impl NotesMutation {
    async fn create_note(
        &self,
        ctx: &Context<'_>,
        input: NoteInput,
    ) -> async_graphql::Result<Note> {
        let library = db(ctx)?;
        let row = notes::create(library, &input.title, &input.content)?;
        let mut tags = tags_by_key(library, &[row.id.clone()], DataType::Note)?;
        let tagged = tags.remove(&row.id).unwrap_or_default();
        Ok(note(row, tagged))
    }

    async fn update_note(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        input: NoteInput,
    ) -> async_graphql::Result<Note> {
        let library = db(ctx)?;
        let row = notes::update(library, id.as_str(), &input.title, &input.content)?;
        let mut tags = tags_by_key(library, &[row.id.clone()], DataType::Note)?;
        let tagged = tags.remove(&row.id).unwrap_or_default();
        Ok(note(row, tagged))
    }

    async fn save_feed_entries_to_notes(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> async_graphql::Result<Vec<async_graphql::ID>> {
        Ok(notes::save_feed_entries(db(ctx)?, &query)?
            .into_iter()
            .map(async_graphql::ID)
            .collect())
    }

    async fn trash_notes(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> async_graphql::Result<ActionResult> {
        Ok(ActionResult {
            affected_count: notes::trash(db(ctx)?, &query)? as i32,
        })
    }

    async fn restore_notes(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> async_graphql::Result<ActionResult> {
        Ok(ActionResult {
            affected_count: notes::restore(db(ctx)?, &query)? as i32,
        })
    }

    /// A blank query would match every note, and a bulk delete is not
    /// something a client should be able to aim at the whole library by
    /// accident — refused before the store is touched.
    async fn delete_notes(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> async_graphql::Result<ActionResult> {
        if query.trim().is_empty() {
            return Err("query is required".into());
        }
        Ok(ActionResult {
            affected_count: notes::delete(db(ctx)?, &query)? as i32,
        })
    }

    async fn export_notes(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> async_graphql::Result<String> {
        Ok(notes::export(db(ctx)?, &query)?)
    }
}

fn note(row: NoteRow, tags: Vec<Tag>) -> Note {
    Note {
        id: row.id.into(),
        title: row.title,
        content: row.content,
        deleted_at: row.deleted_at.as_deref().map(stored_instant),
        created_at: stored_instant(&row.created_at),
        updated_at: stored_instant(&row.updated_at),
        tags,
    }
}

/// Tags for a whole page, keyed by note id. The relations come back in one
/// query and the distinct tag ids are then looked up individually, which is
/// bounded by how many distinct tags a page uses rather than by its length.
pub(super) fn tags_by_key(
    library: &Arc<Db>,
    keys: &[String],
    kind: DataType,
) -> async_graphql::Result<HashMap<String, Vec<Tag>>> {
    if keys.is_empty() {
        return Ok(HashMap::new());
    }
    let relations = tags::relations_for_keys_of_kind(library, keys, kind.kind())?;
    let distinct: HashSet<String> = relations.iter().map(|row| row.tag_id.clone()).collect();
    let mut by_id: HashMap<String, Tag> = HashMap::new();
    for id in distinct {
        if let Some(row) = tags::tag_by_id(library, &id)? {
            by_id.insert(
                row.id.clone(),
                Tag {
                    id: row.id.into(),
                    name: row.name,
                    count: row.count,
                },
            );
        }
    }
    let mut by_key: HashMap<String, Vec<Tag>> = HashMap::new();
    for relation in relations {
        if let Some(tag) = by_id.get(&relation.tag_id) {
            by_key.entry(relation.key).or_default().push(tag.clone());
        }
    }
    Ok(by_key)
}

fn db<'a>(ctx: &'a Context<'_>) -> async_graphql::Result<&'a Arc<Db>> {
    ctx.data::<Arc<Db>>()
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/public_notes.rs"]
mod tests;
