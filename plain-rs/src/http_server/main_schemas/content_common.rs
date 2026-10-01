use super::types::{Instant, Tag};
use async_graphql::{Context, ID};
use chrono::{DateTime, Utc};
use std::sync::Arc;

use crate::db::Db;

pub fn instant(value: &str) -> async_graphql::Result<Instant> {
    let date = DateTime::parse_from_rfc3339(value)
        .map_err(|e| async_graphql::Error::new(format!("invalid stored timestamp: {e}")))?;
    Ok(Instant(date.with_timezone(&Utc)))
}

pub fn tags(ctx: &Context<'_>, id: &str, kind: i32) -> async_graphql::Result<Vec<Tag>> {
    let library = ctx.data::<Arc<Db>>()?;
    Ok(
        crate::library::tags::tags_for_key_of_kind(library, id, kind)
            .into_iter()
            .map(|tag| Tag {
                id: ID(tag.id),
                name: tag.name,
                r#type: tag.kind,
                count: tag.count,
            })
            .collect(),
    )
}
