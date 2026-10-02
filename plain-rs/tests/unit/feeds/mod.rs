use super::*;

fn database() -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("plain.db")).unwrap();
    (dir, db)
}

#[test]
fn parses_rss_and_atom_entries() {
    let rss = r#"<rss><channel><title>News</title><image><title>Logo</title></image><item><title>One</title><link>https://example.org/1</link><description>&lt;p&gt;Hello&lt;/p&gt;</description><pubDate>Wed, 02 Oct 2002 08:00:00 GMT</pubDate></item></channel></rss>"#;
    let parsed = parse_feed(rss).unwrap();
    assert_eq!(parsed.title, "News");
    assert_eq!(parsed.entries[0].url, "https://example.org/1");
    let atom = r#"<feed xmlns="http://www.w3.org/2005/Atom"><title>Atom</title><entry><title>Two</title><link href="https://example.org/2" rel="alternate"/><summary>Summary</summary><published>2026-10-01T12:00:00Z</published></entry></feed>"#;
    let parsed = parse_feed(atom).unwrap();
    assert_eq!(parsed.title, "Atom");
    assert_eq!(parsed.entries[0].url, "https://example.org/2");
}

#[test]
fn article_content_ignores_navigation_and_preserves_markdown() {
    let html = "<html><body><nav>Menu</nav><article><h2>Title</h2><p>Read <a href=\"https://example.org/more\">more</a>.</p></article></body></html>";
    let markdown = html_to_markdown(&article_html(html));
    assert!(markdown.contains("## Title"), "{markdown}");
    assert!(markdown.contains("[more](https://example.org/more)"));
    assert!(!markdown.contains("Menu"));
}

#[test]
fn opml_roundtrip_and_filtered_entry_counts() {
    let (_dir, db) = database();
    let opml = r#"<opml version="2.0"><body><outline text="Group"><outline text="Name" xmlUrl="https://example.org/rss" fetchContent="true"/></outline></body></opml>"#;
    import_opml(&db, opml).unwrap();
    import_opml(&db, opml).unwrap();
    let feeds = db.feeds_list().unwrap();
    assert_eq!(feeds.len(), 1);
    assert_eq!(feeds[0].name, "Name");
    assert!(feeds[0].fetch_content);
    let export = export_opml(&db).unwrap();
    assert!(export.contains("xmlUrl=\"https://example.org/rss\""));
    let now = now();
    let row = FeedEntryRow {
        id: "entry".into(),
        feed_id: feeds[0].id.clone(),
        title: "Needle".into(),
        url: "https://example.org/one".into(),
        image: String::new(),
        description: String::new(),
        author: String::new(),
        content: String::new(),
        raw_id: "hash".into(),
        published_at: now.clone(),
        read: false,
        created_at: now.clone(),
        updated_at: now,
    };
    assert_eq!(db.feed_entries_insert(&[row.clone()]).unwrap().len(), 1);
    assert_eq!(db.feed_entries_insert(&[row]).unwrap().len(), 0);
    assert_eq!(
        count(&db, &format!("feed_id:{} text:Needle", feeds[0].id)).unwrap(),
        1
    );
    assert_eq!(delete_entries(&db, "text:Needle").unwrap(), 1);
    assert_eq!(count(&db, "").unwrap(), 0);
    assert!(delete_entries(&db, "").is_err());
}

#[tokio::test]
async fn sync_fetches_and_deduplicates_entries() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (_dir, db) = database();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/rss", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 1024];
            stream.read(&mut request).await.unwrap();
            let body = "<rss><channel><title>News</title><item><title>One</title><link>https://example.org/one</link></item></channel></rss>";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });
    let feed = db.feed_save("feed", "News", &url, false, &now()).unwrap();
    sync_one(&db, &feed, None).await.unwrap();
    sync_one(&db, &feed, None).await.unwrap();
    assert_eq!(db.feed_entry_count("").unwrap(), 1);
    server.await.unwrap();
}
