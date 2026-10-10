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
    assert_eq!(
        markdown,
        "Title\n-----\n\nRead [more](https://example.org/more)."
    );
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

#[test]
fn feed_entry_markdown_and_relative_assets_contract() {
    let (_dir, db) = database();
    let feed = db
        .feed_save(
            "feed",
            "News",
            "https://example.org/news/rss",
            false,
            &now(),
        )
        .unwrap();
    let parsed = parse_feed(r#"<rss><channel><item><title>&lt;b&gt;News&lt;/b&gt;</title><link>/article</link><description>&lt;p&gt;Fallback&lt;/p&gt;</description><content:encoded xmlns:content="http://purl.org/rss/1.0/modules/content/"><![CDATA[<h2>Title</h2><p>Read <a href="../more">more</a>.</p><img src="/photo.jpg" alt="photo">]]></content:encoded></item></channel></rss>"#).unwrap();
    let row = item_to_row(&feed, parsed.entries.into_iter().next().unwrap());
    assert_eq!(row.title, "**News**");
    assert_eq!(
        row.description,
        "Title\n-----\n\nRead [more](https://example.org/more).\n\n![photo](https://example.org/photo.jpg)"
    );
    assert_eq!(row.image, "https://example.org/photo.jpg");
    assert_eq!(row.url, "https://example.org/article");
}

#[test]
fn article_markdown_retains_code_indentation_and_normalizes_links() {
    let html = "<body><header>Header</header><main><h1>Code</h1><pre><code>  x\n    y</code></pre><p><a href=\"/more\">More</a></p></main><footer>Footer</footer></body>";
    let article = assets::normalize_html(&article_html(html), "https://example.org/article");
    assert_eq!(
        html_to_markdown(&article),
        "Code\n====\n\n      x\n        y\n\n[More](https://example.org/more)"
    );
}

#[test]
fn xml_entities_and_surrounding_spaces_reach_markdown_intact() {
    let parsed = parse_feed("<rss><channel><title> A &amp; B </title><item><title> A &amp; B </title><link> https://example.org/?a=1&amp;b=2 </link><description>&lt;p&gt;A &amp;amp; B &#x4e2d; &#128512;&lt;/p&gt;</description></item></channel></rss>").unwrap();
    assert_eq!(parsed.title, "A & B");
    assert_eq!(parsed.entries[0].url, "https://example.org/?a=1&b=2");
    assert_eq!(html_to_markdown(&parsed.entries[0].title), "A & B");
    assert_eq!(
        html_to_markdown(&parsed.entries[0].description),
        "A & B 中 😀"
    );
    // An undefined named entity is a publisher bug, not a reason to throw the
    // whole feed away — the text survives verbatim.
    assert_eq!(
        parse_feed("<rss><channel><title>A &nbsp; B</title></channel></rss>")
            .unwrap()
            .title,
        "A &nbsp; B"
    );
    // A document with neither a channel title nor an entry is still not a feed.
    assert!(parse_feed("<html><body>not a feed</body></html>").is_err());
}

#[test]
fn the_channel_link_is_the_site_url_and_the_image_link_is_not() {
    // RSS spells the site URL as `<link>text</link>`. `<image><link>` is a
    // different thing: gluing the two together produced a URL that resolves
    // nowhere, which is what favicon discovery then fetched.
    let rss = r#"<rss><channel><title>News</title><link>https://example.org/</link><image><url>/logo.png</url><title>Logo</title><link>https://example.org/</link></image><item><title>One</title></item></channel></rss>"#;
    let parsed = parse_feed(rss).unwrap();
    assert_eq!(parsed.site_url, "https://example.org/");
    assert_eq!(parsed.logo, "/logo.png");

    // Atom spells the same thing as an attribute — without it, every Atom
    // feed looked like it had no homepage.
    let atom = r#"<feed xmlns="http://www.w3.org/2005/Atom"><title>Atom</title><link rel="alternate" href="https://example.org/"/><link rel="self" href="https://example.org/feed.atom"/><entry><title>One</title></entry></feed>"#;
    assert_eq!(parse_feed(atom).unwrap().site_url, "https://example.org/");

    // An Atom link with formatting whitespace still keeps the attribute.
    let padded = r#"<feed><title>Atom</title><link href="https://example.org/padded">  </link><entry><title>One</title></entry></feed>"#;
    assert_eq!(
        parse_feed(padded).unwrap().site_url,
        "https://example.org/padded"
    );
}

#[test]
fn feed_dates_cover_the_formats_publishers_emit() {
    let expected = [
        // Strict RFC 3339 / RFC 2822.
        ("2024-09-10T12:00:00Z", "2024-09-10T12:00:00.000Z"),
        ("2024-09-10T12:00:00.000Z", "2024-09-10T12:00:00.000Z"),
        ("2024-09-10T12:00:00+02:00", "2024-09-10T10:00:00.000Z"),
        ("Tue, 10 Sep 2024 12:00:00 GMT", "2024-09-10T12:00:00.000Z"),
        // Seconds are optional in RFC 822.
        ("Tue, 10 Sep 2024 12:00 GMT", "2024-09-10T12:00:00.000Z"),
        // Two-digit years.
        ("10 Sep 24 12:00:00 GMT", "2024-09-10T12:00:00.000Z"),
        // Day name and single-digit day are both optional.
        ("2 Oct 2024 08:00 GMT", "2024-10-02T08:00:00.000Z"),
        // Named zones, including the ones RFC 2822 dropped.
        ("Wed, 02 Oct 2024 15:30:00 EST", "2024-10-02T20:30:00.000Z"),
        ("Wed, 02 Oct 2002 08:00:00 UT", "2002-10-02T08:00:00.000Z"),
        ("Wed, 02 Oct 2002 08:00:00 Z", "2002-10-02T08:00:00.000Z"),
        // Numeric zones.
        (
            "Tue, 10 Sep 2024 14:00:00 +0200",
            "2024-09-10T12:00:00.000Z",
        ),
        (
            "Tue, 10 Sep 2024 14:00:00 -05:30",
            "2024-09-10T19:30:00.000Z",
        ),
        // No zone at all, and date-only.
        ("2024-09-10 12:00:00", "2024-09-10T12:00:00.000Z"),
        ("2024-09-10T12:00", "2024-09-10T12:00:00.000Z"),
        ("2024-09-10", "2024-09-10T00:00:00.000Z"),
    ];
    for (input, want) in expected {
        assert_eq!(parse_date(input, "fallback"), want, "input: {input}");
    }
    // Unreadable and future dates fall back instead of inventing a value:
    // a future date would otherwise pin the entry to the top forever.
    assert_eq!(parse_date("not a date", "fallback"), "fallback");
    assert_eq!(parse_date("", "fallback"), "fallback");
    assert_eq!(parse_date("2999-01-01T00:00:00Z", "fallback"), "fallback");
}

#[test]
fn opml_import_keeps_the_valid_outlines_when_one_url_is_broken() {
    let (_dir, db) = database();
    let opml = r#"<opml version="2.0"><body>
        <outline text="Group"><outline text="Good" xmlUrl="https://example.org/a" fetchContent="true"/></outline>
        <outline text="Broken" xmlUrl="https://"/>
        <outline text="Also good" xmlUrl="https://example.org/b"/>
        <outline text="Not a url" xmlUrl="example.org/c"/>
    </body></opml>"#;
    import_opml(&db, opml).unwrap();
    let feeds = db.feeds_list().unwrap();
    assert_eq!(feeds.len(), 2);
    assert!(
        feeds
            .iter()
            .any(|feed| feed.url == "https://example.org/a" && feed.fetch_content)
    );
    assert!(feeds.iter().any(|feed| feed.url == "https://example.org/b"));
    // Group outlines carry no xmlUrl and must not become subscriptions.
    assert!(!feeds.iter().any(|feed| feed.name == "Group"));
    // Importing the same file again adds nothing.
    import_opml(&db, opml).unwrap();
    assert_eq!(db.feeds_list().unwrap().len(), 2);
}

#[test]
fn opml_export_escapes_names_and_round_trips_through_the_parser() {
    let (_dir, db) = database();
    db.feed_save(
        "feed",
        "A & B <news>",
        "https://example.org/rss",
        true,
        &now(),
    )
    .unwrap();
    let export = export_opml(&db).unwrap();
    assert!(export.contains("A &amp; B &lt;news&gt;"));
    let (_dir, other) = database();
    import_opml(&other, &export).unwrap();
    let feeds = other.feeds_list().unwrap();
    assert_eq!(feeds.len(), 1);
    assert_eq!(feeds[0].name, "A & B <news>");
    assert!(feeds[0].fetch_content);
}

#[test]
fn absolute_urls_keep_their_scheme_and_drop_the_unsafe_ones() {
    let base = "https://example.org/news/rss";
    assert_eq!(
        assets::absolute_url(base, "../more"),
        Some("https://example.org/more".into())
    );
    assert_eq!(
        assets::absolute_url(base, "/article"),
        Some("https://example.org/article".into())
    );
    assert_eq!(
        assets::absolute_url(base, "https://other.example/x"),
        Some("https://other.example/x".into())
    );
    // Protocol-relative keeps the base scheme.
    assert_eq!(
        assets::absolute_url(base, "//cdn.example/x.png"),
        Some("https://cdn.example/x.png".into())
    );
    // A non-http scheme must stay as it is, never become a site-relative path.
    assert_eq!(assets::absolute_url(base, "javascript:alert(1)"), None);
    assert_eq!(
        assets::absolute_url(base, "data:image/png;base64,AAA"),
        None
    );
    assert_eq!(assets::absolute_url(base, "  "), None);
}

#[test]
fn normalize_html_rewrites_attributes_and_not_text_that_looks_like_one() {
    let html =
        "<p>use <code>href=\"/x\"</code> literally</p><img src=\"/a.png\"><a href=\"../b\">b</a>";
    let normalized = assets::normalize_html(html, "https://example.org/news/post.html");
    assert!(
        normalized.contains(r#"<code>href="/x"</code>"#),
        "{normalized}"
    );
    assert!(
        normalized.contains(r#"src="https://example.org/a.png""#),
        "{normalized}"
    );
    assert!(
        normalized.contains(r#"href="https://example.org/b""#),
        "{normalized}"
    );
    // The link target of the rewritten href is what an entry keeps.
    assert_eq!(
        assets::main_image(&normalized, "https://example.org/news/post.html"),
        Some("https://example.org/a.png".into())
    );
}

#[test]
fn article_html_uses_the_main_container_and_falls_back_to_the_page() {
    // The container's own tag is dropped: only its content is the article.
    assert_eq!(
        article_html(
            "<html><body><nav>Menu</nav><div role=\"main\"><p>Kept</p></div><footer>Dropped</footer></body></html>"
        ),
        "<p>Kept</p>"
    );
    // No container at all: the page itself is the article.
    assert_eq!(
        article_html("<div><p>Plain body</p></div>"),
        "<div><p>Plain body</p></div>"
    );
}

#[test]
fn atom_attribute_link_is_not_polluted_by_formatting_whitespace() {
    let parsed = parse_feed("<feed><title>Atom</title><entry><link href=\"https://example.org/a?x=1&amp;y=2\">  </link><summary>&lt;p&gt;A &amp;amp; B&lt;/p&gt;</summary></entry></feed>").unwrap();
    assert_eq!(parsed.entries[0].url, "https://example.org/a?x=1&y=2");
    assert_eq!(html_to_markdown(&parsed.entries[0].description), "A & B");
}

#[test]
fn missing_feed_links_and_images_remain_empty() {
    let (_dir, db) = database();
    let feed = db
        .feed_save("feed", "News", "https://example.org/rss", false, &now())
        .unwrap();
    let row = item_to_row(
        &feed,
        ParsedEntry {
            title: "No assets".into(),
            ..ParsedEntry::default()
        },
    );
    assert_eq!(row.url, "");
    assert_eq!(row.image, "");
    assert_eq!(assets::absolute_url(&feed.url, ""), None);
    assert_eq!(assets::absolute_url(&feed.url, "  "), None);
}

#[tokio::test]
async fn create_fetches_channel_title_without_preview_api() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (_dir, db) = database();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/rss", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 1024];
        stream.read(&mut request).await.unwrap();
        let body = "<rss><channel><title>A &amp; B</title></channel></rss>";
        stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .as_bytes(),
            )
            .await
            .unwrap();
    });
    let feed = create_without_sync(&db, &url, false).await.unwrap();
    assert_eq!(feed.name, "A & B");
    assert_eq!(db.feed_entry_count("").unwrap(), 0);
    assert!(create_without_sync(&db, &url, false).await.is_err());
    assert!(
        create_without_sync(&db, "file:///tmp/test", false)
            .await
            .is_err()
    );
    server.await.unwrap();
}
