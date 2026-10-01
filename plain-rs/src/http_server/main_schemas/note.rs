use super::content_common;
use super::types::{ActionResult, Instant, Tag};
use async_graphql::{ComplexObject, Context, ID, InputObject, Object, SimpleObject};
use std::sync::Arc;

use crate::enums::DataType;
use crate::db::{Db, notes_feeds::NoteRow};
use crate::notes;

type GqlResult<T> = async_graphql::Result<T>;

#[derive(SimpleObject, Clone)]
#[graphql(complex)]
pub struct Note {
    pub id: ID,
    pub title: String,
    pub content: String,
    pub deleted_at: Option<Instant>,
    pub created_at: Instant,
    pub updated_at: Instant,
}

#[ComplexObject]
impl Note {
    async fn tags(&self, ctx: &Context<'_>) -> GqlResult<Vec<Tag>> {
        content_common::tags(ctx, self.id.as_str(), DataType::Note.kind())
    }
}

#[derive(InputObject)]
pub struct NoteInput {
    pub title: String,
    pub content: String,
}

fn to_model(row: NoteRow) -> GqlResult<Note> {
    Ok(Note {
        id: ID(row.id),
        title: row.title,
        content: row.content,
        deleted_at: row
            .deleted_at
            .as_deref()
            .map(content_common::instant)
            .transpose()?,
        created_at: content_common::instant(&row.created_at)?,
        updated_at: content_common::instant(&row.updated_at)?,
    })
}

#[derive(Default)]
pub struct NoteQuery;

#[Object]
impl NoteQuery {
    async fn note_count(&self, ctx: &Context<'_>, query: String) -> GqlResult<i32> {
        Ok(notes::count(ctx.data::<Arc<Db>>()?, &query)?)
    }

    async fn note(&self, ctx: &Context<'_>, id: ID) -> GqlResult<Option<Note>> {
        notes::get(ctx.data::<Arc<Db>>()?, id.as_str())?
            .map(to_model)
            .transpose()
    }

    async fn notes(
        &self,
        ctx: &Context<'_>,
        offset: i32,
        limit: i32,
        query: String,
    ) -> GqlResult<Vec<Note>> {
        notes::search(ctx.data::<Arc<Db>>()?, &query, limit, offset)?
            .into_iter()
            .map(to_model)
            .collect()
    }
}

#[derive(Default)]
pub struct NoteMutation;

#[Object]
impl NoteMutation {
    async fn create_note(&self, ctx: &Context<'_>, input: NoteInput) -> GqlResult<Note> {
        to_model(notes::create(
            ctx.data::<Arc<Db>>()?,
            &input.title,
            &input.content,
        )?)
    }

    async fn update_note(&self, ctx: &Context<'_>, id: ID, input: NoteInput) -> GqlResult<Note> {
        to_model(notes::update(
            ctx.data::<Arc<Db>>()?,
            id.as_str(),
            &input.title,
            &input.content,
        )?)
    }

    async fn save_feed_entries_to_notes(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> GqlResult<Vec<ID>> {
        Ok(
            notes::save_feed_entries(ctx.data::<Arc<Db>>()?, &query)?
                .into_iter()
                .map(ID)
                .collect(),
        )
    }

    async fn trash_notes(&self, ctx: &Context<'_>, query: String) -> GqlResult<ActionResult> {
        Ok(ActionResult {
            affected_count: notes::trash(ctx.data::<Arc<Db>>()?, &query)? as i32,
        })
    }

    async fn restore_notes(&self, ctx: &Context<'_>, query: String) -> GqlResult<ActionResult> {
        Ok(ActionResult {
            affected_count: notes::restore(ctx.data::<Arc<Db>>()?, &query)? as i32,
        })
    }

    async fn delete_notes(&self, ctx: &Context<'_>, query: String) -> GqlResult<ActionResult> {
        Ok(ActionResult {
            affected_count: notes::delete(ctx.data::<Arc<Db>>()?, &query)? as i32,
        })
    }

    async fn export_notes(&self, ctx: &Context<'_>, query: String) -> GqlResult<String> {
        Ok(notes::export(ctx.data::<Arc<Db>>()?, &query)?)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/api/schema/note.rs"]
mod tests;
