//! Unit tests for `src/xml_sax.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

#[test]
fn parse_simple_element() {
    let xml = b"<foo>bar</foo>";
    let mut r = Reader::new(xml);
    assert_eq!(r.next_event(), Some(Event::Start { name: "foo".into() }));
    assert_eq!(
        r.next_event(),
        Some(Event::Text {
            content: "bar".into()
        })
    );
    assert_eq!(r.next_event(), Some(Event::End { name: "foo".into() }));
    assert_eq!(r.next_event(), None);
}

#[test]
fn parse_nested_with_self_closing() {
    let xml = b"<root><a/><b>hi</b></root>";
    let mut r = Reader::new(xml);
    assert_eq!(
        r.next_event(),
        Some(Event::Start {
            name: "root".into()
        })
    );
    assert_eq!(r.next_event(), Some(Event::Start { name: "a".into() }));
    assert_eq!(r.next_event(), Some(Event::End { name: "a".into() }));
    assert_eq!(r.next_event(), Some(Event::Start { name: "b".into() }));
    assert_eq!(
        r.next_event(),
        Some(Event::Text {
            content: "hi".into()
        })
    );
    assert_eq!(r.next_event(), Some(Event::End { name: "b".into() }));
    assert_eq!(
        r.next_event(),
        Some(Event::End {
            name: "root".into()
        })
    );
    assert_eq!(r.next_event(), None);
}

#[test]
fn parse_strips_namespace_prefix() {
    let xml = b"<dc:friendlyName xmlns:dc=\"urn:dc\">X</dc:friendlyName>";
    let mut r = Reader::new(xml);
    assert_eq!(
        r.next_event(),
        Some(Event::Start {
            name: "friendlyName".into()
        })
    );
    assert_eq!(
        r.next_event(),
        Some(Event::Text {
            content: "X".into()
        })
    );
    assert_eq!(
        r.next_event(),
        Some(Event::End {
            name: "friendlyName".into()
        })
    );
}

#[test]
fn decode_entities_basic() {
    assert_eq!(decode_entities(b"a &amp; b"), "a & b");
    assert_eq!(decode_entities(b"&lt;tag&gt;"), "<tag>");
    assert_eq!(decode_entities(b"&quot;hi&quot;"), "\"hi\"");
}

#[test]
fn parse_skips_comments() {
    let xml = b"<a><!-- skip -->b</a>";
    let mut r = Reader::new(xml);
    assert_eq!(r.next_event(), Some(Event::Start { name: "a".into() }));
    assert_eq!(
        r.next_event(),
        Some(Event::Text {
            content: "b".into()
        })
    );
    assert_eq!(r.next_event(), Some(Event::End { name: "a".into() }));
}
