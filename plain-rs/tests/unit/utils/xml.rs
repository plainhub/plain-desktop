use super::*;

fn events(input: &str) -> Vec<Event> {
    let mut reader = Reader::new(input);
    let mut out = Vec::new();
    while let Some(event) = reader.next() {
        out.push(event);
    }
    out
}

fn tags(input: &str) -> Vec<String> {
    events(input)
        .into_iter()
        .filter_map(|event| match event {
            Event::Start(tag) if tag.empty => Some(format!("<{}/>", tag.name)),
            Event::Start(tag) => Some(format!("<{}>", tag.name)),
            Event::End(name) => Some(format!("</{name}>")),
            Event::Text(_) => None,
        })
        .collect()
}

fn text(input: &str) -> String {
    events(input)
        .into_iter()
        .filter_map(|event| match event {
            Event::Text(text) => Some(text),
            _ => None,
        })
        .collect()
}

#[test]
fn elements_attributes_and_self_closing_tags() {
    assert_eq!(
        tags(r#"<?xml version="1.0"?><rss version='2.0'><channel><link/><item/></channel></rss>"#),
        [
            "<rss>",
            "<channel>",
            "<link/>",
            "<item/>",
            "</channel>",
            "</rss>"
        ]
    );
    let mut reader = Reader::new(
        r#"<outline text="Group" xmlUrl='https://a.example/rss' fetchContent="true" bare=unquoted/>"#,
    );
    let Event::Start(tag) = reader.next().unwrap() else {
        panic!("expected a start tag");
    };
    assert!(tag.empty);
    assert_eq!(tag.name, "outline");
    assert_eq!(tag.attr("xmlUrl"), Some("https://a.example/rss"));
    assert_eq!(tag.attr("fetchContent"), Some("true"));
    assert_eq!(tag.attr("bare"), Some("unquoted"));
    assert_eq!(tag.attr("missing"), None);
}

#[test]
fn namespace_prefixes_are_stripped_from_names_and_attributes() {
    let mut reader = Reader::new(
        r#"<content:encoded><atom:link atom:href="https://a.example/"/></content:encoded>"#,
    );
    let Event::Start(tag) = reader.next().unwrap() else {
        panic!("expected a start tag");
    };
    assert_eq!(tag.name, "encoded");
    let Event::Start(link) = reader.next().unwrap() else {
        panic!("expected a start tag");
    };
    assert_eq!(link.name, "link");
    assert_eq!(link.attr("href"), Some("https://a.example/"));
}

#[test]
fn cdata_comments_and_declarations_do_not_leak_into_text() {
    assert_eq!(
        events("<rss><!-- note --><![CDATA[<p>raw & <b>bold</b></p>]]></rss>"),
        [
            Event::Start(StartTag {
                name: "rss".into(),
                attrs: vec![],
                empty: false
            }),
            Event::Text("<p>raw & <b>bold</b></p>".into()),
            Event::End("rss".into())
        ]
    );
    // What follows the CDATA marker must not be glued onto its content.
    assert_eq!(text("<t><![CDATA[a]]><b>c</b></t>"), "ac");
    assert_eq!(text("<t><!DOCTYPE html><?pi go?>keep</t>"), "keep");
}

#[test]
fn entities_decode_but_undefined_ones_stay_verbatim() {
    assert_eq!(
        decode("A &amp; B &lt;c&gt; &quot;q&quot; &apos;a&apos;"),
        "A & B <c> \"q\" 'a'"
    );
    assert_eq!(decode("&#x4e2d; &#128512; &#65;"), "中 😀 A");
    assert_eq!(decode("A &nbsp; B"), "A &nbsp; B");
    assert_eq!(decode("5 & 6 & 7;"), "5 & 6 & 7;");
    assert_eq!(decode("plain"), "plain");
    let parsed = events("<title>A &unknown; B</title>");
    assert!(parsed.contains(&Event::Text("A &unknown; B".into())));
}

#[test]
fn attribute_values_decode_entities() {
    let mut reader = Reader::new(r#"<link href="https://a.example/?a=1&amp;b=2"/>"#);
    let Event::Start(tag) = reader.next().unwrap() else {
        panic!("expected a start tag");
    };
    assert_eq!(tag.attr("href"), Some("https://a.example/?a=1&b=2"));
}

#[test]
fn broken_markup_recovers_instead_of_failing_the_document() {
    // Mismatched end tag closes the elements it actually opened.
    assert_eq!(tags("<a><b>x</a>"), ["<a>", "<b>", "</b>", "</a>"]);
    // Unclosed elements close when the input ends.
    assert_eq!(tags("<a><b>"), ["<a>", "<b>", "</b>", "</a>"]);
    // Stray end tags are ignored.
    assert_eq!(tags("</a><b>"), ["<b>", "</b>"]);
    // A '<' that opens nothing is text, not a parse failure.
    assert_eq!(text("<t>5 < 6</t>"), "5 < 6");
    // Truncated input never panics or loops.
    assert_eq!(tags("<a"), ["<a>", "</a>"]);
    assert_eq!(tags("<a attr="), ["<a>", "</a>"]);
    assert_eq!(text("<>"), "<>");
}

#[test]
fn escape_marks_the_five_characters_an_attribute_cannot_carry() {
    assert_eq!(escape(r#"a&b<c>d"e'f"#), "a&amp;b&lt;c&gt;d&quot;e&apos;f");
}
