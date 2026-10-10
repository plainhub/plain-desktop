use crate::db::{
    Db,
    notes_feeds::{FeedEntryRow, FeedRow},
};
use crate::error::{DomainError, DomainResult};
use crate::utils::{
    http_url::parse_http_url,
    xml::{self, Event},
};
use chrono::Utc;

pub fn now() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn error(message: impl ToString) -> DomainError {
    DomainError::Other(message.to_string())
}

pub fn require_url(url: &str) -> DomainResult<()> {
    let parsed = parse_http_url(url).ok_or_else(|| error("invalid_feed_url"))?;
    if parsed.host.is_empty() {
        return Err(error("invalid_feed_url"));
    }
    Ok(())
}

pub fn update(db: &Db, id: &str, name: &str, fetch_content: bool) -> DomainResult<FeedRow> {
    let feed = db
        .feed_get(id)?
        .ok_or_else(|| error(format!("Feed {id} not found")))?;
    Ok(db.feed_save(id, name, &feed.url, fetch_content, &now())?)
}

pub fn delete(db: &Db, id: &str) -> DomainResult<bool> {
    Ok(db.feed_delete(id)?)
}

pub fn list(db: &Db) -> DomainResult<Vec<FeedRow>> {
    Ok(db.feeds_list()?)
}

pub fn get(db: &Db, id: &str) -> DomainResult<Option<FeedRow>> {
    Ok(db.feed_get(id)?)
}

pub fn entry_get(db: &Db, id: &str) -> DomainResult<Option<FeedEntryRow>> {
    Ok(db.feed_entry_get(id)?)
}

pub fn entry_counts(db: &Db) -> DomainResult<Vec<(String, i32)>> {
    Ok(db.feed_entry_counts()?)
}

pub fn search(db: &Db, query: &str, limit: i32, offset: i32) -> DomainResult<Vec<FeedEntryRow>> {
    Ok(db.feed_entries_list(query, i64::from(limit.max(0)), i64::from(offset.max(0)))?)
}

pub fn count(db: &Db, query: &str) -> DomainResult<i32> {
    Ok(db.feed_entry_count(query)?)
}

pub fn delete_entries(db: &Db, query: &str) -> DomainResult<usize> {
    if query.trim().is_empty() {
        return Err(error(
            "query is required for bulk mutations — pass 'all:true' to explicitly target everything",
        ));
    }
    Ok(db.feed_entries_delete(query)?)
}

pub fn update_url(db: &Db, id: &str, url: &str) -> DomainResult<FeedRow> {
    require_url(url)?;
    let feed = db.feed_get(id)?.ok_or_else(|| error("feed_not_found"))?;
    Ok(db.feed_save(id, &feed.name, url, feed.fetch_content, &now())?)
}

pub fn mark_read(db: &Db, query: &str, read: bool) -> DomainResult<usize> {
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

pub fn import_opml(db: &Db, content: &str) -> DomainResult<()> {
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

pub fn export_opml(db: &Db) -> DomainResult<String> {
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

pub mod date;
