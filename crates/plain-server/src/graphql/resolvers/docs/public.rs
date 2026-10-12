//! Public `/graphql` media-library roots: buckets, and the image / video /
//! document lists with their counts.
//!
//! The catalogue lives in the platform's media store, so every row and every
//! count here is a host fact — Rust cannot enumerate a MediaStore. What Rust
//! does own is the contract shape and the degradation rule: plain-app shows
//! an empty library rather than an error when storage is not granted, while
//! an explicit browse still errors out, because a silently empty page and a
//! denied page mean different things to the caller.

use crate::graphql::resolvers::tags::domain_types::Tag;
use crate::graphql::types::Instant;
use crate::graphql::types::Long;
use crate::graphql::types::facts::id;
use crate::graphql::types::facts::integer;
use crate::graphql::types::facts::rows;
use crate::graphql::types::facts::text;
use crate::http::auth::public_gate;
use async_graphql::{Context, Enum, Object, SimpleObject};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::{collections::HashMap, path::Path, sync::Arc};

use crate::graphql::resolvers::media::public::STORAGE;
use crate::graphql::resolvers::media::public::host_call;
use crate::graphql::resolvers::media::public::media_count;
use crate::graphql::resolvers::media::public::media_rows;
use crate::graphql::resolvers::media::public::prefs;
use crate::graphql::resolvers::media::public::row;

#[derive(SimpleObject, Clone, Debug)]
#[graphql(complex)]
pub struct Doc {
    pub id: async_graphql::ID,
    pub title: String,
    pub path: String,
    pub extension: String,
    pub size: Long,
    #[graphql(name = "bucketId")]
    pub bucket_id: async_graphql::ID,
    #[graphql(name = "createdAt")]
    pub created_at: Instant,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
}

#[async_graphql::ComplexObject]
impl Doc {
    async fn tags(&self, ctx: &async_graphql::Context<'_>) -> async_graphql::Result<Vec<Tag>> {
        crate::graphql::resolvers::tags::loader::load(
            ctx,
            plain_rs::enums::DataType::Doc,
            self.id.as_str(),
        )
        .await
    }
}

#[derive(SimpleObject, Clone, Debug)]
pub struct DocExtGroup {
    pub ext: String,
    pub count: i32,
}

#[derive(Default)]
pub struct DocsQuery;

#[Object]
impl DocsQuery {
    async fn doc_count(&self, ctx: &Context<'_>, query: String) -> async_graphql::Result<i32> {
        if !public_gate::require_granted(ctx, &[STORAGE]).await? {
            return Ok(0);
        }
        media_count(ctx, "DOC", query).await
    }

    async fn docs(
        &self,
        ctx: &Context<'_>,
        query: String,
        offset: i32,
        limit: i32,
        sort_by: crate::graphql::resolvers::files::types::FileSortBy,
    ) -> async_graphql::Result<Vec<Doc>> {
        public_gate::require(ctx, prefs(ctx), &[STORAGE])?;
        let items = media_rows(
            ctx,
            "systemMediaRows",
            json!({
                "dataType": "DOC", "query": query,
                "offset": offset, "limit": limit, "sortBy": sort_by.as_str(),
            }),
        )
        .await?;
        Ok(rows(&items, |item| {
            let row = row(item);
            Doc {
                id: row.id,
                title: row.title,
                // Derived from the path, exactly as plain-app's lazy
                // `DDoc.extension` does — the store does not carry it.
                extension: Path::new(&row.path)
                    .extension()
                    .map(|value| value.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                path: row.path,
                size: row.size,
                bucket_id: row.bucket_id,
                created_at: row.created_at,
                updated_at: row.updated_at,
            }
        }))
    }

    async fn doc_ext_groups(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<DocExtGroup>> {
        if !public_gate::require_granted(ctx, &[STORAGE]).await? {
            return Ok(Vec::new());
        }
        let facts = host_call(ctx, "systemDocExtGroups", json!({})).await?;
        Ok(rows(&facts, |item| DocExtGroup {
            ext: text(item, "ext"),
            count: integer(item, "count") as i32,
        }))
    }
}
