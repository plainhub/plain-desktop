//! Public `/graphql` media-library roots: buckets, and the image / video /
//! document lists with their counts.
//!
//! The catalogue lives in the platform's media store, so every row and every
//! count here is a host fact — Rust cannot enumerate a MediaStore. What Rust
//! does own is the contract shape and the degradation rule: plain-app shows
//! an empty library rather than an error when storage is not granted, while
//! an explicit browse still errors out, because a silently empty page and a
//! denied page mean different things to the caller.

use super::host::Host;
use super::public_contact_types::Tag;
use super::public_facts::{flag, id, integer, list, optional_instant, rows, text};
use super::public_gate;
use crate::content_types::{Instant, Long};
use async_graphql::{Context, Enum, Object, SimpleObject};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::{collections::HashMap, path::Path, sync::Arc};

const STORAGE: &str = "WRITE_EXTERNAL_STORAGE";

/// The library kind a client is asking about. The names are the contract's,
/// which is also what the platform's `DataType` enum uses — the mapping is
/// the identity, so there is nothing to translate.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum MediaDataType {
    Audio,
    Video,
    Image,
    Doc,
}

impl MediaDataType {
    fn as_str(self) -> &'static str {
        match self {
            MediaDataType::Audio => "AUDIO",
            MediaDataType::Video => "VIDEO",
            MediaDataType::Image => "IMAGE",
            MediaDataType::Doc => "DOC",
        }
    }
}

#[derive(SimpleObject, Clone, Debug)]
pub struct MediaBucket {
    pub id: async_graphql::ID,
    pub name: String,
    #[graphql(name = "itemCount")]
    pub item_count: i32,
    #[graphql(name = "topItemPaths")]
    pub top_item_paths: Vec<String>,
}

/// The fields every library row shares. Kept as a helper rather than a
/// GraphQL interface: the contract declares `MediaItem` as an interface, but
/// nothing in it can be queried on its own — the three concrete types each
/// add fields and are what the lists return.
struct Row {
    id: async_graphql::ID,
    title: String,
    path: String,
    size: Long,
    bucket_id: async_graphql::ID,
    created_at: Instant,
    updated_at: Instant,
}

fn row(item: &Value) -> Row {
    Row {
        id: id(item, "id"),
        title: text(item, "title"),
        path: text(item, "path"),
        size: Long(integer(item, "size")),
        bucket_id: id(item, "bucketId"),
        created_at: instant(item, "createdAt"),
        updated_at: instant(item, "updatedAt"),
    }
}

#[derive(SimpleObject, Clone, Debug)]
pub struct Image {
    pub id: async_graphql::ID,
    pub title: String,
    pub path: String,
    pub size: Long,
    #[graphql(name = "bucketId")]
    pub bucket_id: async_graphql::ID,
    #[graphql(name = "createdAt")]
    pub created_at: Instant,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
    #[graphql(name = "takenAt")]
    pub taken_at: Option<Instant>,
    #[graphql(name = "isFavorite")]
    pub is_favorite: bool,
    pub tags: Vec<Tag>,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct Video {
    pub id: async_graphql::ID,
    pub title: String,
    pub path: String,
    #[graphql(name = "durationMs")]
    pub duration_ms: Long,
    pub size: Long,
    #[graphql(name = "bucketId")]
    pub bucket_id: async_graphql::ID,
    #[graphql(name = "createdAt")]
    pub created_at: Instant,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
    #[graphql(name = "takenAt")]
    pub taken_at: Option<Instant>,
    #[graphql(name = "isFavorite")]
    pub is_favorite: bool,
    pub tags: Vec<Tag>,
}

#[derive(SimpleObject, Clone, Debug)]
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
    pub tags: Vec<Tag>,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct DocExtGroup {
    pub ext: String,
    pub count: i32,
}

#[derive(Default)]
pub struct MediaQuery;

#[Object]
impl MediaQuery {
    /// Folder counts for one library kind. Degrades to empty rather than
    /// erroring: a client showing the folder strip has nothing useful to do
    /// with `no_permission`, but an empty strip reads as "no folders yet".
    async fn media_buckets(
        &self,
        ctx: &Context<'_>,
        r#type: MediaDataType,
    ) -> async_graphql::Result<Vec<MediaBucket>> {
        if !public_gate::require_granted(ctx, &[STORAGE]).await? {
            return Ok(Vec::new());
        }
        let facts = host_call(
            ctx,
            "systemMediaBucketItemFacts",
            json!({ "dataType": r#type.as_str() }),
        )
        .await?;
        Ok(aggregate_buckets(&facts)
            .into_iter()
            .map(Bucket::into_media)
            .collect())
    }

    async fn image_count(&self, ctx: &Context<'_>, query: String) -> async_graphql::Result<i32> {
        if !public_gate::require_granted(ctx, &[STORAGE]).await? {
            return Ok(0);
        }
        let query_text = text_field(&query);
        let count = host_call(
            ctx,
            "systemImageCount",
            json!({ "queryText": query_text, "extraQuery": query }),
        )
        .await?;
        Ok(count.as_i64().unwrap_or_default() as i32)
    }

    async fn images(
        &self,
        ctx: &Context<'_>,
        offset: i32,
        limit: i32,
        query: String,
        sort_by: crate::content_types::FileSortBy,
    ) -> async_graphql::Result<Vec<Image>> {
        public_gate::require(prefs(ctx), &[STORAGE])?;
        let items = media_rows(
            ctx,
            "systemImageRows",
            json!({
                "queryText": text_field(&query),
                "extraQuery": query,
                "offset": offset,
                "limit": limit,
                "sortBy": sort_by.as_str(),
            }),
        )
        .await?;
        let tags = tags_for(ctx, "IMAGE", &items).await?;
        Ok(rows(&items, |item| {
            let row = row(item);
            Image {
                id: row.id,
                title: row.title,
                path: row.path,
                size: row.size,
                bucket_id: row.bucket_id,
                created_at: row.created_at,
                updated_at: row.updated_at,
                taken_at: optional_instant(item, "takenAt"),
                is_favorite: flag(item, "isFavorite"),
                tags: tags
                    .get(&id(item, "id").to_string())
                    .cloned()
                    .unwrap_or_default(),
            }
        }))
    }

    async fn video_count(&self, ctx: &Context<'_>, query: String) -> async_graphql::Result<i32> {
        if !public_gate::require_granted(ctx, &[STORAGE]).await? {
            return Ok(0);
        }
        media_count(ctx, "VIDEO", query).await
    }

    async fn videos(
        &self,
        ctx: &Context<'_>,
        offset: i32,
        limit: i32,
        query: String,
        sort_by: crate::content_types::FileSortBy,
    ) -> async_graphql::Result<Vec<Video>> {
        public_gate::require(prefs(ctx), &[STORAGE])?;
        let items = media_rows(
            ctx,
            "systemMediaRows",
            json!({
                "dataType": "VIDEO", "query": query,
                "offset": offset, "limit": limit, "sortBy": sort_by.as_str(),
            }),
        )
        .await?;
        let tags = tags_for(ctx, "VIDEO", &items).await?;
        Ok(rows(&items, |item| {
            let row = row(item);
            Video {
                id: row.id,
                title: row.title,
                path: row.path,
                duration_ms: Long(integer(item, "durationMs")),
                size: row.size,
                bucket_id: row.bucket_id,
                created_at: row.created_at,
                updated_at: row.updated_at,
                taken_at: optional_instant(item, "takenAt"),
                is_favorite: flag(item, "isFavorite"),
                tags: tags
                    .get(&id(item, "id").to_string())
                    .cloned()
                    .unwrap_or_default(),
            }
        }))
    }

    async fn doc_count(&self, ctx: &Context<'_>, query: String) -> async_graphql::Result<i32> {
        if !public_gate::require_granted(ctx, &[STORAGE]).await? {
            return Ok(0);
        }
        media_count(ctx, "DOC", query).await
    }

    async fn docs(
        &self,
        ctx: &Context<'_>,
        offset: i32,
        limit: i32,
        query: String,
        sort_by: crate::content_types::FileSortBy,
    ) -> async_graphql::Result<Vec<Doc>> {
        public_gate::require(prefs(ctx), &[STORAGE])?;
        let items = media_rows(
            ctx,
            "systemMediaRows",
            json!({
                "dataType": "DOC", "query": query,
                "offset": offset, "limit": limit, "sortBy": sort_by.as_str(),
            }),
        )
        .await?;
        let tags = tags_for(ctx, "DOC", &items).await?;
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
                tags: tags
                    .get(&id(item, "id").to_string())
                    .cloned()
                    .unwrap_or_default(),
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

/// Groups rows into folders and orders them by the lowercased folder name,
/// the same rule plain-app's bucket bar uses. The platform reports one row
/// per item; the contract wants one row per folder.
struct Bucket {
    id: String,
    name: String,
    sort_name: String,
    item_count: i32,
    top_item_paths: Vec<String>,
}

impl Bucket {
    fn into_media(self) -> MediaBucket {
        MediaBucket {
            id: async_graphql::ID::from(self.id),
            name: self.name,
            item_count: self.item_count,
            top_item_paths: self.top_item_paths,
        }
    }
}

/// At most this many sample paths per folder — enough for a thumbnail
/// grid, and a bound so one huge folder cannot drag every path over the
/// wire.
const BUCKET_SAMPLES: usize = 4;

fn aggregate_buckets(value: &Value) -> Vec<Bucket> {
    let mut buckets: HashMap<String, Bucket> = HashMap::new();
    for item in rows(value, |item| item.clone()) {
        let key = text(&item, "id");
        if key.is_empty() {
            continue;
        }
        let path = text(&item, "path");
        match buckets.get_mut(&key) {
            Some(existing) => {
                existing.item_count += 1;
                if existing.top_item_paths.len() < BUCKET_SAMPLES {
                    existing.top_item_paths.push(path);
                }
            }
            None => {
                buckets.insert(
                    key.clone(),
                    Bucket {
                        id: key,
                        name: text(&item, "name"),
                        sort_name: text(&item, "sortName").to_lowercase(),
                        item_count: 1,
                        top_item_paths: vec![path],
                    },
                );
            }
        }
    }
    let mut aggregated: Vec<Bucket> = buckets.into_values().collect();
    aggregated.sort_by(|a, b| a.sort_name.cmp(&b.sort_name).then_with(|| a.id.cmp(&b.id)));
    aggregated
}

/// The DSL's bare-token name filter, pulled out on its own: plain-app splits
/// `text` off the query and hands the rest to the platform as the
/// structured filter, so the platform never sees the text twice.
fn text_field(query: &str) -> String {
    crate::utils::search_dsl::parse(query)
        .into_iter()
        .find(|field| field.name == "text")
        .map(|field| field.value)
        .unwrap_or_default()
}

/// One page of library rows, still in the host's raw JSON shape.
async fn media_rows(
    ctx: &Context<'_>,
    method: &str,
    params: Value,
) -> async_graphql::Result<Value> {
    host_call(ctx, method, params).await
}

async fn media_count(
    ctx: &Context<'_>,
    data_type: &str,
    query: String,
) -> async_graphql::Result<i32> {
    let count = host_call(
        ctx,
        "systemMediaCount",
        json!({ "dataType": data_type, "query": query }),
    )
    .await?;
    Ok(count.as_i64().unwrap_or_default() as i32)
}

/// Tags are per-row, so they are fetched once for the page and keyed by the
/// media id. The platform answers from the same Rust store the roots read
/// from, so the ids have to survive the round trip unchanged.
async fn tags_for(
    ctx: &Context<'_>,
    data_type: &str,
    items: &Value,
) -> async_graphql::Result<HashMap<String, Vec<Tag>>> {
    let keys: Vec<String> = rows(items, |item| id(item, "id").to_string())
        .into_iter()
        .filter(|key| !key.is_empty())
        .collect();
    if keys.is_empty() {
        return Ok(Default::default());
    }
    let facts = host_call(
        ctx,
        "systemMediaTagFacts",
        json!({ "dataType": data_type, "keys": keys }),
    )
    .await?;
    let mut tags = HashMap::new();
    for item in rows(&facts, |item| item.clone()) {
        tags.insert(
            text(&item, "key"),
            list(&item, "tags", |tag| Tag {
                id: id(tag, "id"),
                name: text(tag, "name"),
                count: integer(tag, "count") as i32,
            }),
        );
    }
    Ok(tags)
}

fn instant(value: &Value, key: &str) -> Instant {
    optional_instant(value, key)
        .unwrap_or_else(|| Instant(DateTime::<Utc>::from_timestamp(0, 0).unwrap_or_default()))
}

async fn host_call(ctx: &Context<'_>, method: &str, params: Value) -> async_graphql::Result<Value> {
    ctx.data_unchecked::<Arc<Host>>()
        .call(method, params)
        .await
        .map_err(async_graphql::Error::new)
}

fn prefs<'a>(ctx: &'a Context<'_>) -> &'a Arc<crate::prefs::Prefs> {
    ctx.data_unchecked::<Arc<crate::prefs::Prefs>>()
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/public_media.rs"]
mod tests;
