//! DIDL-Lite metadata builder. We don't need a full XML encoder for the
//! few fields we emit, so we hand-build the payload as a string. The
//! shape is byte-for-byte compatible with Go's `didlLiteMetadata`.

use super::types::MediaType;
use super::util::xml_escape;

/// Build a DIDL-Lite `<DIDL-Lite>` document describing a single media
/// item. Returned as a `String` (UTF-8 XML), suitable for inclusion in
/// the `CurrentURIMetaData` argument of `SetAVTransportURI`.
pub fn didl_lite_metadata(
    media_url: &str,
    title: &str,
    mime: &str,
    media_type: MediaType,
) -> String {
    let upnp_class = match media_type {
        MediaType::Video => "object.item.videoItem",
        MediaType::Audio => "object.item.audioItem.musicTrack",
        MediaType::Image => "object.item.imageItem.photo",
    };

    let title = if title.trim().is_empty() {
        "PlainNAS"
    } else {
        title
    };
    let mime = if mime.trim().is_empty() {
        "application/octet-stream"
    } else {
        mime
    };

    let title_esc = xml_escape(title);
    let url_esc = xml_escape(media_url);

    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>\
<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/" \
xmlns:dc="http://purl.org/dc/elements/1.1/" \
xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/" \
xmlns:dlna="urn:schemas-dlna-org:metadata-1-0/">\
<item id="0" parentID="0" restricted="1">\
<dc:title>{title}</dc:title>\
<upnp:class>{class}</upnp:class>\
<res protocolInfo="http-get:*:{mime}:*">{url}</res>\
</item>\
</DIDL-Lite>"#,
        title = title_esc,
        class = upnp_class,
        mime = mime,
        url = url_esc,
    )
}

#[cfg(test)]
#[path = "../../../tests/unit/nas/dlna/didl.rs"]
mod tests;
