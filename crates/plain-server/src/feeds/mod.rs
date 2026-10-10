use crate::db::Db;
use crate::db::notes_feeds::{FeedEntryRow, FeedRow};
use crate::library::{LibraryError, LibraryResult};
use crate::utils::html_to_markdown::dom::{Dom, Node};
use crate::utils::html_to_markdown::html_to_markdown;
use crate::utils::http_url::parse_http_url;
use crate::utils::xml::{self, Event, StartTag};
use crate::ws_event::WsEvent;
use chrono::Utc;
use futures_util::StreamExt;
use futures_util::stream;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Default)]
struct ParsedFeed {
    title: String,
    entries: Vec<ParsedEntry>,
    logo: String,
    site_url: String,
    /// Atom states the site URL as `<link href>`; once taken, the whitespace
    /// inside that tag must not be appended to it as if it were the URL.
    site_url_from_attribute: bool,
}

#[derive(Default)]
struct ParsedEntry {
    raw_id: String,
    title: String,
    url: String,
    url_from_attribute: bool,
    image: String,
    description: String,
    content: String,
    author: String,
    published_at: String,
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn error(message: impl ToString) -> LibraryError {
    LibraryError::Other(message.to_string())
}

fn require_url(url: &str) -> LibraryResult<()> {
    let parsed = parse_http_url(url).ok_or_else(|| error("invalid_feed_url"))?;
    if parsed.host.is_empty() {
        return Err(error("invalid_feed_url"));
    }
    Ok(())
}

fn inside(stack: &[String], name: &str) -> bool {
    stack.iter().any(|open| open == name)
}

/// Serialized children of a node — the body without the node's own tag.
fn inner_html(dom: &Dom, id: usize) -> String {
    dom.0[id]
        .children
        .iter()
        .map(|&child| dom.outer_html(child))
        .collect()
}

fn parse_feed(xml: &str) -> LibraryResult<ParsedFeed> {
    let mut reader = xml::Reader::new(xml);
    let mut feed = ParsedFeed::default();
    let mut entry: Option<ParsedEntry> = None;
    let mut stack: Vec<String> = Vec::new();
    while let Some(event) = reader.next() {
        match event {
            Event::Start(tag) => {
                if tag.name == "item" || tag.name == "entry" {
                    if tag.empty {
                        feed.entries.push(ParsedEntry::default());
                    } else {
                        entry = Some(ParsedEntry::default());
                    }
                } else if let Some(item) = entry.as_mut() {
                    assign_item_tag(item, &tag);
                } else {
                    assign_feed_tag(&mut feed, &stack, &tag);
                }
                if !tag.empty {
                    stack.push(tag.name);
                }
            }
            Event::Text(text) => {
                assign_text(&mut feed, entry.as_mut(), &stack, &text);
            }
            Event::End(name) => {
                if (name == "item" || name == "entry")
                    && let Some(mut item) = entry.take()
                {
                    item.url = item.url.trim().to_string();
                    item.raw_id = item.raw_id.trim().to_string();
                    item.author = item.author.trim().to_string();
                    item.published_at = item.published_at.trim().to_string();
                    feed.entries.push(item);
                }
                stack.pop();
            }
        }
    }
    feed.title = feed.title.trim().to_string();
    feed.logo = feed.logo.trim().to_string();
    feed.site_url = feed.site_url.trim().to_string();
    if feed.title.is_empty() && feed.entries.is_empty() {
        return Err(error("invalid_feed_content"));
    }
    Ok(feed)
}

fn assign_item_tag(item: &mut ParsedEntry, tag: &StartTag) {
    if tag.name == "link" {
        if let Some(href) = tag.attr("href") {
            let rel = tag.attr("rel").unwrap_or_default();
            if rel.is_empty() || rel == "alternate" {
                item.url = href.to_string();
                item.url_from_attribute = true;
            }
        }
    }
    if matches!(tag.name.as_str(), "thumbnail" | "enclosure" | "content") {
        if let Some(url) = tag.attr("url") {
            item.image = url.to_string();
        }
    }
}

fn assign_feed_tag(feed: &mut ParsedFeed, stack: &[String], tag: &StartTag) {
    if tag.name != "link" || inside(stack, "image") || !feed.site_url.is_empty() {
        return;
    }
    let rel = tag.attr("rel").unwrap_or_default();
    if !rel.is_empty() && rel != "alternate" {
        return;
    }
    if let Some(href) = tag.attr("href") {
        feed.site_url.push_str(href);
        feed.site_url_from_attribute = true;
    }
}

fn assign_text(
    feed: &mut ParsedFeed,
    entry: Option<&mut ParsedEntry>,
    stack: &[String],
    text: &str,
) {
    let name = stack.last().map(String::as_str).unwrap_or("");
    if let Some(item) = entry {
        match name {
            "title" => item.title.push_str(text),
            "guid" | "id" => item.raw_id.push_str(text),
            "link" => {
                if !item.url_from_attribute {
                    item.url.push_str(text);
                }
            }
            "description" | "summary" => item.description.push_str(text),
            "encoded" | "content" => item.content.push_str(text),
            "creator" | "author" | "name" => item.author.push_str(text),
            "pubdate" | "published" | "updated" | "date" => item.published_at.push_str(text),
            _ => {}
        }
    } else if name == "url" && inside(stack, "image") {
        feed.logo.push_str(text);
    } else if name == "link" && !inside(stack, "image") && !feed.site_url_from_attribute {
        // RSS puts the site URL in `<link>text</link>`. `<image><link>` is a
        // different thing and must not be glued onto the channel URL.
        feed.site_url.push_str(text);
    } else if name == "title"
        && stack
            .get(stack.len().saturating_sub(2))
            .is_some_and(|parent| parent == "channel" || parent == "feed")
    {
        feed.title.push_str(text);
    }
}

fn parse_date(text: &str, fallback: &str) -> String {
    date::parse(text)
        .filter(|date| date.with_timezone(&Utc) < Utc::now())
        .map(|date| {
            date.with_timezone(&Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        })
        .unwrap_or_else(|| fallback.to_string())
}

/// Picks the article body out of a fetched page: the first `<article>`, else
/// `<main>`, else a `role="main"` container, else `<body>`. Pages that have
/// none of them (minimal HTML, fragments) are used as they are.
fn article_html(html: &str) -> String {
    let dom = Dom::parse(html);
    let pick = |wanted: &dyn Fn(&Node) -> bool| (1..dom.0.len()).find(|&id| wanted(&dom.0[id]));
    let found = pick(&|node| node.tag == "article")
        .or_else(|| pick(&|node| node.tag == "main"))
        .or_else(|| pick(&|node| node.attr("role") == "main"))
        .or_else(|| pick(&|node| node.tag == "body"));
    match found {
        Some(id) => inner_html(&dom, id),
        None => inner_html(&dom, 0),
    }
}

fn item_to_row(feed: &FeedRow, item: ParsedEntry) -> FeedEntryRow {
    let at = now();
    let fallback = uuid::Uuid::new_v4().to_string();
    let raw_key = if !item.raw_id.is_empty() {
        item.raw_id.as_str()
    } else if !item.url.is_empty() {
        item.url.as_str()
    } else if !item.title.is_empty() {
        item.title.as_str()
    } else {
        fallback.as_str()
    };
    let raw_id = crate::utils::hex::bytes_to_hex(&Sha256::digest(
        format!("{}_{}", feed.id, raw_key).as_bytes(),
    ));
    let image = assets::absolute_url(&feed.url, &item.image)
        .or_else(|| assets::main_image(&item.content, &feed.url))
        .or_else(|| assets::main_image(&item.description, &feed.url))
        .unwrap_or_default();
    let description = if item.content.is_empty() {
        &item.description
    } else {
        &item.content
    };
    FeedEntryRow {
        id: uuid::Uuid::new_v4().to_string(),
        feed_id: feed.id.clone(),
        title: html_to_markdown(&item.title),
        url: assets::absolute_url(&feed.url, &item.url).unwrap_or(item.url),
        image,
        description: html_to_markdown(&assets::normalize_html(description, &feed.url)),
        author: item.author,
        content: String::new(),
        raw_id,
        published_at: parse_date(&item.published_at, &at),
        read: false,
        created_at: at.clone(),
        updated_at: at,
    }
}

pub(crate) async fn fetch_text(url: &str, max_bytes: usize) -> LibraryResult<String> {
    require_url(url)?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(25))
        .build()
        .map_err(error)?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(error)?
        .error_for_status()
        .map_err(|e| {
            error(format!(
                "{}: {e}",
                if e.is_timeout() {
                    "TIMEOUT"
                } else if e.status().is_some() {
                    "SERVER"
                } else if e.is_connect() {
                    "DNS"
                } else {
                    "UNKNOWN"
                }
            ))
        })?;
    if response
        .content_length()
        .is_some_and(|size| size > max_bytes as u64)
    {
        return Err(error("response_too_large"));
    }
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(error)?;
        if bytes.len() + chunk.len() > max_bytes {
            return Err(error("response_too_large"));
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(error)
}

pub async fn create(
    db: Arc<Db>,
    url: &str,
    fetch_content: bool,
    events: broadcast::Sender<WsEvent>,
) -> LibraryResult<FeedRow> {
    if db.feed_get_by_url(url)?.is_some() {
        return Err(error("feed_already_exists"));
    }
    let feed = create_without_sync(&db, url, fetch_content).await?;
    let id = feed.id.clone();
    queue_sync(db, events, Some(id));
    Ok(feed)
}

pub fn update(db: &Db, id: &str, name: &str, fetch_content: bool) -> LibraryResult<FeedRow> {
    let feed = db
        .feed_get(id)?
        .ok_or_else(|| error(format!("Feed {id} not found")))?;
    Ok(db.feed_save(id, name, &feed.url, fetch_content, &now())?)
}

pub fn delete(db: &Db, id: &str) -> LibraryResult<bool> {
    Ok(db.feed_delete(id)?)
}

pub fn list(db: &Db) -> LibraryResult<Vec<FeedRow>> {
    Ok(db.feeds_list()?)
}

pub fn get(db: &Db, id: &str) -> LibraryResult<Option<FeedRow>> {
    Ok(db.feed_get(id)?)
}

pub fn entry_get(db: &Db, id: &str) -> LibraryResult<Option<FeedEntryRow>> {
    Ok(db.feed_entry_get(id)?)
}

pub fn entry_counts(db: &Db) -> LibraryResult<Vec<(String, i32)>> {
    Ok(db.feed_entry_counts()?)
}

pub fn search(db: &Db, query: &str, limit: i32, offset: i32) -> LibraryResult<Vec<FeedEntryRow>> {
    Ok(db.feed_entries_list(query, i64::from(limit.max(0)), i64::from(offset.max(0)))?)
}

pub fn count(db: &Db, query: &str) -> LibraryResult<i32> {
    Ok(db.feed_entry_count(query)?)
}

pub fn delete_entries(db: &Db, query: &str) -> LibraryResult<usize> {
    if query.trim().is_empty() {
        return Err(error(
            "query is required for bulk mutations — pass 'all:true' to explicitly target everything (API_SPEC §5)",
        ));
    }
    Ok(db.feed_entries_delete(query)?)
}

pub async fn sync_entry_content(db: &Db, id: &str) -> LibraryResult<FeedEntryRow> {
    sync_entry_content_with_assets(db, id, None).await
}
pub async fn sync_entry_content_with_assets(
    db: &Db,
    id: &str,
    assets: Option<&FeedAssets>,
) -> LibraryResult<FeedEntryRow> {
    let entry = db
        .feed_entry_get(id)?
        .ok_or_else(|| error(format!("Feed entry {id} not found")))?;
    {
        let html = fetch_text(&entry.url, 4 * 1024 * 1024).await?;
        let article = assets::normalize_html(&article_html(&html), &entry.url);
        let content = html_to_markdown(&article);
        if let Some(assets) = assets {
            if !entry.image.starts_with('/') {
                if let Some(url) = if entry.image.is_empty() {
                    assets::main_image(&article, &entry.url)
                } else {
                    Some(entry.image.clone())
                } {
                    if let Ok(image) = assets.cache(&url).await {
                        db.feed_entry_set_image(id, &image)?;
                    }
                }
            }
        }
        if content.len() >= entry.description.len() && !content.is_empty() {
            db.feed_entry_set_content(id, &content, &now())?;
        }
    }
    Ok(db.feed_entry_get(id)?.unwrap_or(entry))
}

async fn sync_one(db: &Db, feed: &FeedRow, assets: Option<&FeedAssets>) -> LibraryResult<()> {
    let xml = fetch_text(&feed.url, 4 * 1024 * 1024).await?;
    let parsed = parse_feed(&xml)?;
    let rows: Vec<_> = parsed
        .entries
        .into_iter()
        .map(|item| item_to_row(feed, item))
        .collect();
    let inserted = db.feed_entries_insert(&rows)?;
    if feed.fetch_content {
        stream::iter(inserted)
            .for_each_concurrent(4, |entry| async move {
                let _ = sync_entry_content_with_assets(db, &entry.id, assets).await;
            })
            .await;
    }
    if let Some(assets) = assets {
        if feed.logo.is_empty() {
            let mut candidates = Vec::new();
            if let Some(url) =
                assets::absolute_url(&feed.url, &parsed.logo).filter(|_| !parsed.logo.is_empty())
            {
                candidates.push(url);
            }
            let site = if parsed.site_url.is_empty() {
                feed.url.clone()
            } else {
                parsed.site_url
            };
            if let Ok(html) = fetch_text(&site, 2 * 1024 * 1024).await {
                let dom = Dom::parse(&html);
                for node in &dom.0 {
                    if node.tag != "link" || !node.attr("rel").contains("icon") {
                        continue;
                    }
                    if let Some(url) = assets::absolute_url(&site, node.attr("href")) {
                        candidates.push(url);
                    }
                }
            }
            for url in candidates {
                if let Ok(logo) = assets.cache(&url).await {
                    db.feed_set_logo(&feed.id, &logo)?;
                    break;
                }
            }
        }
        for entry in db.feed_entries_list(&format!("feed_id:{}", feed.id), i64::MAX, 0)? {
            if entry.image.starts_with("http") {
                if let Ok(image) = assets.cache(&entry.image).await {
                    db.feed_entry_set_image(&entry.id, &image)?;
                }
            }
        }
    }
    Ok(())
}

pub async fn sync(
    db: Arc<Db>,
    events: broadcast::Sender<WsEvent>,
    id: Option<String>,
) -> LibraryResult<()> {
    sync_with_assets(db, events, id, None).await
}
pub async fn sync_with_assets(
    db: Arc<Db>,
    events: broadcast::Sender<WsEvent>,
    id: Option<String>,
    assets: Option<Arc<FeedAssets>>,
) -> LibraryResult<()> {
    let feeds = match id.as_deref() {
        Some(id) => vec![db.feed_get(id)?.ok_or_else(|| error("feed_not_found"))?],
        None => db.feeds_list()?,
    };
    let errors: Vec<String> = stream::iter(feeds)
        .map(|feed| {
            let db = db.clone();
            let events = events.clone();
            let assets = assets.clone();
            async move {
                let result = sync_one(&db, &feed, assets.as_deref()).await;
                let detail = result
                    .as_ref()
                    .err()
                    .map(ToString::to_string)
                    .unwrap_or_default();
                let code = if detail.is_empty() {
                    ""
                } else if detail.contains("TIMEOUT:") || detail.contains("timed out") {
                    "TIMEOUT"
                } else if detail.contains("SERVER:") {
                    "SERVER"
                } else if detail.contains("DNS:") {
                    "DNS"
                } else if detail.contains("invalid_feed_content")
                    || detail.contains("decode")
                    || detail.contains("XML")
                {
                    "PARSE"
                } else {
                    "UNKNOWN"
                };
                let stored = serde_json::json!({"code":code,"detail":detail}).to_string();
                let persist = db.feed_set_sync_status(&feed.id, &now(), &stored);
                let detail = persist.err().map(|e| e.to_string()).unwrap_or(detail);
                let _ = events.send(WsEvent::broadcast(
                    "FEEDS_FETCHED",
                    serde_json::json!({"feedId":feed.id,"error":detail}).to_string(),
                ));
                detail
            }
        })
        .buffer_unordered(16)
        .filter(|e| std::future::ready(!e.is_empty()))
        .collect()
        .await;
    if id.is_none() {
        let _ = events.send(WsEvent::broadcast(
            "FEEDS_FETCHED",
            serde_json::json!({"feedId":"all","error":errors.join("\n")}).to_string(),
        ));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(error(errors.join("\n")))
    }
}
pub fn queue_sync(db: Arc<Db>, events: broadcast::Sender<WsEvent>, id: Option<String>) {
    tokio::spawn(async move {
        let _ = sync(db, events, id).await;
    });
}
pub fn update_url(db: &Db, id: &str, url: &str) -> LibraryResult<FeedRow> {
    require_url(url)?;
    let feed = db.feed_get(id)?.ok_or_else(|| error("feed_not_found"))?;
    Ok(db.feed_save(id, &feed.name, url, feed.fetch_content, &now())?)
}
pub fn mark_read(db: &Db, query: &str, read: bool) -> LibraryResult<usize> {
    if query.trim().is_empty() {
        return Err(error("query is required"));
    }
    let ids = db
        .feed_entries_list(query, i64::MAX, 0)?
        .into_iter()
        .map(|e| e.id)
        .collect::<Vec<_>>();
    Ok(db.feed_entries_mark_read(&ids, read)?)
}

pub fn import_opml(db: &Db, content: &str) -> LibraryResult<()> {
    let mut reader = xml::Reader::new(content);
    let mut feeds = Vec::new();
    while let Some(event) = reader.next() {
        let Event::Start(tag) = event else {
            continue;
        };
        if tag.name != "outline" {
            continue;
        }
        // Outlines without xmlUrl are the folders that group subscriptions.
        let Some(url) = tag.attr("xmlUrl") else {
            continue;
        };
        let name = tag
            .attr("title")
            .or_else(|| tag.attr("text"))
            .unwrap_or_default()
            .to_string();
        let fetch_content = tag.attr("fetchContent") == Some("true");
        feeds.push((name, url.to_string(), fetch_content));
    }
    for (name, url, fetch_content) in feeds {
        // One malformed outline must not cost the user the rest of the file:
        // OPML is hand-edited and exported by plenty of other readers.
        if require_url(&url).is_err() || db.feed_get_by_url(&url)?.is_some() {
            continue;
        }
        db.feed_save(
            &uuid::Uuid::new_v4().to_string(),
            &name,
            &url,
            fetch_content,
            &now(),
        )?;
    }
    Ok(())
}

pub fn export_opml(db: &Db) -> LibraryResult<String> {
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><opml version=\"2.0\"><head><title>PlainApp</title><dateCreated>{}</dateCreated></head><body>",
        now()
    );
    for feed in db.feeds_list()? {
        xml.push_str(&format!(
            "<outline text=\"{}\" title=\"{}\" xmlUrl=\"{}\" fetchContent=\"{}\"/>",
            xml::escape(&feed.name),
            xml::escape(&feed.name),
            xml::escape(&feed.url),
            feed.fetch_content
        ));
    }
    xml.push_str("</body></opml>");
    Ok(xml)
}

#[cfg(test)]
#[path = "../../tests/unit/feeds/mod.rs"]
mod tests;

mod date;
mod sync_service;
pub use sync_service::{FeedSyncState, SyncService};
pub async fn create_without_sync(
    db: &Db,
    url: &str,
    fetch_content: bool,
) -> LibraryResult<FeedRow> {
    if db.feed_get_by_url(url)?.is_some() {
        return Err(error("feed_already_exists"));
    }
    require_url(url)?;
    let title = parse_feed(&fetch_text(url, 4 * 1024 * 1024).await?)?.title;
    Ok(db.feed_save(
        &uuid::Uuid::new_v4().to_string(),
        &title,
        url,
        fetch_content,
        &now(),
    )?)
}

pub(crate) mod assets;
pub use assets::FeedAssets;
