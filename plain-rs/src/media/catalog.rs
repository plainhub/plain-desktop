use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{Result, anyhow};

use crate::media::image_index::{self, MediaSearchResult, MediaSort};
use crate::media::kv::Db;
use crate::media::scan;

pub async fn search_page(
    query: &str,
    media_type: Option<&str>,
    sort: MediaSort,
    offset: i32,
    limit: i32,
    metadata_db: Option<Arc<Db>>,
    hydrate_audio: bool,
) -> Result<Vec<MediaSearchResult>> {
    let mut rows = image_index::global().search(
        query,
        media_type,
        None,
        sort,
        offset.max(0) as usize,
        limit.clamp(1, 500) as usize,
    )?;
    if let Some(db) = metadata_db {
        rows = tokio::task::spawn_blocking(move || {
            scan::hydrate_search_page(&db, &mut rows, hydrate_audio);
            rows
        })
        .await
        .map_err(|error| anyhow!("blocking task failed: {error}"))?;
    }
    Ok(rows)
}

pub fn count(query: &str, media_type: Option<&str>) -> i32 {
    match image_index::global().count(query, media_type, None) {
        Ok(count) => count.min(i32::MAX as usize) as i32,
        Err(error) => {
            log::error!("[gql] media count failed: {error}");
            0
        }
    }
}

pub async fn doc_ext_groups() -> Result<Vec<(String, i64)>> {
    tokio::task::spawn_blocking(|| image_index::global().doc_ext_groups())
        .await
        .map_err(|error| anyhow!("join: {error}"))?
        .map_err(|error| anyhow!("docExtGroups: {error}"))
}

pub async fn buckets(
    db: Arc<Db>,
    media_type: String,
) -> Result<(Vec<scan::MediaBucketInfo>, HashMap<String, Vec<String>>)> {
    tokio::task::spawn_blocking(move || -> Result<_> {
        let buckets = scan::list_buckets(&db, &media_type)?;
        let wanted: HashSet<String> = buckets.iter().map(|bucket| bucket.dir.clone()).collect();
        let tops = image_index::global()
            .bucket_top_items(&media_type, &wanted, 4)
            .unwrap_or_default();
        Ok((buckets, tops))
    })
    .await
    .map_err(|error| anyhow!("join error: {error}"))?
}
