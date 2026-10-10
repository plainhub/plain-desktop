//! Unit tests for `src/dlna/didl.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

#[test]
fn video_metadata_has_correct_class() {
    let xml = didl_lite_metadata(
        "http://x/y.mp4",
        "Big Buck Bunny",
        "video/mp4",
        MediaType::Video,
    );
    assert!(xml.contains("<upnp:class>object.item.videoItem</upnp:class>"));
    assert!(xml.contains("http-get:*:video/mp4:*"));
    assert!(xml.contains(">http://x/y.mp4</res>"));
    assert!(xml.contains("<dc:title>Big Buck Bunny</dc:title>"));
}

#[test]
fn audio_metadata_uses_music_track() {
    let xml = didl_lite_metadata("http://x/y.mp3", "Song", "audio/mpeg", MediaType::Audio);
    assert!(xml.contains("<upnp:class>object.item.audioItem.musicTrack</upnp:class>"));
    assert!(xml.contains("http-get:*:audio/mpeg:*"));
}

#[test]
fn image_metadata_uses_photo_class() {
    let xml = didl_lite_metadata("http://x/y.jpg", "P", "image/jpeg", MediaType::Image);
    assert!(xml.contains("<upnp:class>object.item.imageItem.photo</upnp:class>"));
}

#[test]
fn empty_title_falls_back_to_plainnas() {
    let xml = didl_lite_metadata("http://x/y", "   ", "video/mp4", MediaType::Video);
    assert!(xml.contains("<dc:title>PlainNAS</dc:title>"));
}

#[test]
fn empty_mime_falls_back_to_octet_stream() {
    let xml = didl_lite_metadata("http://x/y", "T", " ", MediaType::Video);
    assert!(xml.contains("http-get:*:application/octet-stream:*"));
}

#[test]
fn special_chars_are_escaped() {
    let xml = didl_lite_metadata("http://x/<a>&b\"c", "Q&R", "video/mp4", MediaType::Video);
    assert!(xml.contains("&lt;a&gt;&amp;b&quot;c"));
    assert!(xml.contains("<dc:title>Q&amp;R</dc:title>"));
    // The unescaped `<` must NOT appear in the URL position.
    assert!(!xml.contains(">http://x/<a>&b\"c</res>"));
}

#[test]
fn mobile_metadata_is_valid_xml_with_album_art_and_unknown_class() {
    let xml = metadata(
        "http://host/file?x=1&y=2",
        "A & <B>",
        "audio/mpeg",
        MediaType::Unknown,
        "http://host/art?a=1&b=2",
        true,
    );
    let mut elements = Vec::new();
    let mut reader = crate::utils::xml::Reader::new(&xml);
    while let Some(event) = reader.next() {
        if let crate::utils::xml::Event::Start(tag) = event {
            elements.push((tag.name.clone(), tag.attrs.clone()));
        }
    }
    // The markup has to survive a real parse, not just look right.
    assert!(elements.iter().any(|(name, _)| name == "res"));
    assert!(elements.iter().any(|(_, attrs)| {
        attrs
            .iter()
            .any(|(key, value)| key == "parentID" && value == "-1")
    }));
    assert!(xml.contains("object.item</upnp:class>"));
    assert!(xml.contains("A &amp; &lt;B&gt;"));
    assert!(xml.contains("http://host/art?a=1&amp;b=2"));
    assert!(xml.contains("parentID=\"-1\""));
    assert!(!xml.contains("\\\""));
}
