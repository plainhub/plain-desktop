//! Public `/graphql` note roots.
//!
//! Notes are Rust SQLite rows, so the whole surface is local. The app's own
//! note roots could not be mounted here because their `tags` field is the
//! desktop `Tag`, which carries a numeric kind the contract does not have —
//! and two types claiming `Tag` in one registry is a build-time failure, not
//! a merge conflict.

use crate::graphql::resolvers::tags::types::Tag;
use crate::graphql::types::ActionResult;
use crate::graphql::types::Instant;
use crate::graphql::types::facts::stored_instant;
use async_graphql::{Context, InputObject, Object, SimpleObject};
use plain_rs::db::Db;
use plain_rs::db::notes_feeds::NoteRow;
use plain_rs::notes;
use std::sync::Arc;

#[derive(SimpleObject, Clone, Debug)]
#[graphql(complex)]
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
}

#[async_graphql::ComplexObject]
impl Note {
    async fn tags(&self, ctx: &async_graphql::Context<'_>) -> async_graphql::Result<Vec<Tag>> {
        crate::graphql::resolvers::tags::loader::load(
            ctx,
            plain_rs::enums::DataType::Note,
            self.id.as_str(),
        )
        .await
    }
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
        Ok(Some(note(row)))
    }

    async fn notes(
        &self,
        ctx: &Context<'_>,
        query: String,
        offset: i32,
        limit: i32,
    ) -> async_graphql::Result<Vec<Note>> {
        let library = db(ctx)?;
        let rows = notes::search(library, &query, limit, offset)?;
        Ok(rows.into_iter().map(|row| note(row)).collect())
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
        Ok(note(row))
    }

    async fn update_note(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        input: NoteInput,
    ) -> async_graphql::Result<Note> {
        let library = db(ctx)?;
        let row = notes::update(library, id.as_str(), &input.title, &input.content)?;
        Ok(note(row))
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

fn note(row: NoteRow) -> Note {
    Note {
        id: row.id.into(),
        title: row.title,
        content: row.content,
        deleted_at: row.deleted_at.as_deref().map(stored_instant),
        created_at: stored_instant(&row.created_at),
        updated_at: stored_instant(&row.updated_at),
    }
}

fn db<'a>(ctx: &'a Context<'_>) -> async_graphql::Result<&'a Arc<Db>> {
    ctx.data::<Arc<Db>>()
}

#[cfg(test)]
#[path = "../../../../tests/unit/graphql/resolvers/notes/public.rs"]
mod tests;
