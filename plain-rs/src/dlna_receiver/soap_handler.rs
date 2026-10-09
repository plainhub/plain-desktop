use crate::dlna_receiver::types::DlnaMediaType;

const ENVELOPE_NS: &str = "http://schemas.xmlsoap.org/soap/envelope/";
const ENCODING_NS: &str = "http://schemas.xmlsoap.org/soap/encoding/";
const AVT_SERVICE_TYPE: &str = "urn:schemas-upnp-org:service:AVTransport:1";

pub fn response_envelope(action: &str, elements: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="{ENVELOPE_NS}" s:encodingStyle="{ENCODING_NS}">
  <s:Body>
    <u:{action}Response xmlns:u="{AVT_SERVICE_TYPE}">
      {elements}
    </u:{action}Response>
  </s:Body>
</s:Envelope>"#
    )
}

pub fn build_response(action: &str, elements: &str) -> String {
    response_envelope(action, elements)
}

pub fn build_transport_info_response(transport_state: &str) -> String {
    build_response(
        "GetTransportInfo",
        &format!(
            "<CurrentTransportState>{transport_state}</CurrentTransportState>\
             <CurrentTransportStatus>OK</CurrentTransportStatus><CurrentSpeed>1</CurrentSpeed>"
        ),
    )
}

pub fn build_position_info_response(
    rel_time: &str,
    track_duration: &str,
    track_uri: &str,
) -> String {
    build_response(
        "GetPositionInfo",
        &format!(
            "<Track>1</Track><TrackDuration>{track_duration}</TrackDuration>\
             <TrackMetaData>NOT_IMPLEMENTED</TrackMetaData><TrackURI>{}</TrackURI>\
             <RelTime>{rel_time}</RelTime><AbsTime>NOT_IMPLEMENTED</AbsTime>\
             <RelCount>2147483647</RelCount><AbsCount>2147483647</AbsCount>",
            xml_escape(track_uri)
        ),
    )
}

pub fn build_media_info_response() -> String {
    build_response(
        "GetMediaInfo",
        "<NrTracks>1</NrTracks><MediaDuration>00:00:00</MediaDuration>\
         <CurrentURI></CurrentURI><CurrentURIMetaData></CurrentURIMetaData>\
         <PlayMedium>NONE</PlayMedium><RecordMedium>NOT_IMPLEMENTED</RecordMedium>\
         <WriteStatus>NOT_IMPLEMENTED</WriteStatus>",
    )
}

pub fn parse_soap_action(
    soap_action_header: &str,
    body: &str,
) -> (String, std::collections::HashMap<String, String>) {
    let action = soap_action_header
        .trim_matches('"')
        .rsplit('#')
        .next()
        .unwrap_or("")
        .to_string();
    let params = parse_body_params(body, &action);
    (action, params)
}

fn parse_body_params(xml: &str, action: &str) -> std::collections::HashMap<String, String> {
    let mut result = std::collections::HashMap::new();
    let bytes = xml.as_bytes();
    let len = bytes.len();

    // Find the action element opening tag
    let mut pos = 0;
    let action_inner_start = loop {
        if pos >= len {
            break None;
        }
        let tag_start = match bytes[pos..].iter().position(|&b| b == b'<') {
            Some(p) => pos + p,
            None => break None,
        };
        let tag_end = match bytes[tag_start..].iter().position(|&b| b == b'>') {
            Some(p) => tag_start + p,
            None => break None,
        };
        let tag_content = &xml[tag_start + 1..tag_end];
        if !tag_content.starts_with('/')
            && !tag_content.starts_with('!')
            && !tag_content.starts_with('?')
        {
            let tag_name = tag_content
                .split([' ', '\t', '\n', '\r', '/', '>'])
                .next()
                .unwrap_or("");
            if tag_name.ends_with(action) {
                break Some(tag_end + 1);
            }
        }
        pos = tag_end + 1;
    };

    let Some(mut pos) = action_inner_start else {
        return result;
    };

    loop {
        if pos >= len {
            break;
        }
        let tag_start = match bytes[pos..].iter().position(|&b| b == b'<') {
            Some(p) => pos + p,
            None => break,
        };
        let tag_end = match bytes[tag_start..].iter().position(|&b| b == b'>') {
            Some(p) => tag_start + p,
            None => break,
        };
        let tag_content = &xml[tag_start + 1..tag_end];

        if let Some(stripped) = tag_content.strip_prefix('/') {
            let close_tag = stripped
                .split([' ', '\t', '\n', '\r', '>'])
                .next()
                .unwrap_or("");
            if close_tag.ends_with(action) {
                break;
            }
            pos = tag_end + 1;
            continue;
        }

        if tag_content.ends_with('/')
            || tag_content.starts_with('!')
            || tag_content.starts_with('?')
        {
            pos = tag_end + 1;
            continue;
        }

        let tag_name = tag_content
            .split([' ', '\t', '\n', '\r', '/', '>'])
            .next()
            .unwrap_or("")
            .to_string();
        let text_start = tag_end + 1;
        let text_end = match bytes[text_start..].iter().position(|&b| b == b'<') {
            Some(p) => text_start + p,
            None => break,
        };
        let text = &xml[text_start..text_end];
        result.insert(tag_name, decode_xml_entities(text));
        pos = text_end + 1;
    }

    result
}

pub fn extract_title_from_didl_meta(meta: &str) -> String {
    let start = meta.find("<dc:title>");
    let end = meta.find("</dc:title>");
    match (start, end) {
        (Some(s), Some(e)) if e > s => meta[s + 10..e]
            .trim()
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'"),
        _ => String::new(),
    }
}

pub fn extract_album_art_uri_from_didl_meta(meta: &str) -> String {
    let Some(start) = meta.find("<upnp:albumArtURI") else {
        return String::new();
    };
    let Some(tag_end_rel) = meta[start..].find('>') else {
        return String::new();
    };
    let tag_end = start + tag_end_rel;
    let Some(close_rel) = meta[tag_end..].find("</upnp:albumArtURI>") else {
        return String::new();
    };
    meta[tag_end + 1..tag_end + close_rel].trim().to_string()
}

pub fn clean_media_title(raw: &str) -> String {
    let media_extensions = [
        "mp3", "flac", "aac", "ogg", "m4a", "wav", "opus", "wma", "mp4", "mkv", "avi", "mov",
        "wmv", "flv", "ts", "webm", "jpg", "jpeg", "png", "gif", "webp", "bmp", "heic", "heif",
    ];
    let decoded = percent_decode(raw);
    let ext = decoded.rsplit('.').next().unwrap_or("").to_lowercase();
    if media_extensions.contains(&ext.as_str()) {
        decoded
            .rsplitn(2, '.')
            .last()
            .unwrap_or(&decoded)
            .to_string()
    } else {
        decoded
    }
}

pub fn extract_media_type_from_didl_meta(meta: &str, fallback_uri: &str) -> DlnaMediaType {
    // Tier 1: protocolInfo MIME
    if let Some(mime) = extract_protocol_info_mime(meta) {
        let cls = classify_by_mime(&mime);
        if cls != DlnaMediaType::UNKNOWN {
            return cls;
        }
    }
    // Tier 2: upnp:class
    let class_start = meta.find("<upnp:class>");
    let class_end = meta.find("</upnp:class>");
    if let (Some(cs), Some(ce)) = (class_start, class_end)
        && cs < ce
    {
        let cls = meta[cs + 12..ce].to_lowercase();
        return if cls.contains("audioitem") || cls.contains("musictrack") {
            DlnaMediaType::AUDIO
        } else if cls.contains("imageitem") || cls.contains("photo") {
            DlnaMediaType::IMAGE
        } else if cls.contains("videoitem") {
            DlnaMediaType::VIDEO
        } else {
            DlnaMediaType::UNKNOWN
        };
    }
    // Tier 3: URL extension
    let ext = fallback_uri
        .rsplit('.')
        .next()
        .unwrap_or("")
        .split('?')
        .next()
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "mp3" | "flac" | "aac" | "ogg" | "m4a" | "wav" | "opus" | "wma" => DlnaMediaType::AUDIO,
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "heic" | "heif" => DlnaMediaType::IMAGE,
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "ts" | "webm" => DlnaMediaType::VIDEO,
        _ => DlnaMediaType::UNKNOWN,
    }
}

fn extract_protocol_info_mime(didl_meta: &str) -> Option<String> {
    let tag_start = didl_meta.find("<res")?;
    let tag_end_rel = didl_meta[tag_start..].find('>')?;
    let tag_attrs = &didl_meta[tag_start..tag_start + tag_end_rel];
    let pi_start = tag_attrs.find("protocolInfo=\"")?;
    let after = &tag_attrs[pi_start + 15..];
    let pi_end = after.find('"')?;
    let pi = &after[..pi_end];
    let parts: Vec<&str> = pi.split(':').collect();
    if parts.len() >= 3 {
        Some(parts[2].to_string())
    } else {
        None
    }
}

fn classify_by_mime(mime: &str) -> DlnaMediaType {
    if mime.starts_with("video/") {
        DlnaMediaType::VIDEO
    } else if mime.starts_with("audio/") {
        DlnaMediaType::AUDIO
    } else if mime.starts_with("image/") {
        DlnaMediaType::IMAGE
    } else {
        DlnaMediaType::UNKNOWN
    }
}

pub fn parse_dlna_time_to_ms(time: &str) -> i64 {
    let parts: Vec<&str> = time.split(':').collect();
    if parts.len() >= 3 {
        let h = parts[0].parse::<i64>().unwrap_or(-1);
        let m = parts[1].parse::<i64>().unwrap_or(-1);
        let s = parts[2]
            .split('.')
            .next()
            .unwrap_or("0")
            .parse::<i64>()
            .unwrap_or(-1);
        if h >= 0 && m >= 0 && s >= 0 {
            return (h * 3600 + m * 60 + s) * 1000;
        }
    }
    -1
}

pub fn resolve_sender_name(
    headers: &std::collections::HashMap<String, String>,
    sender_ip: &str,
) -> String {
    headers
        .get("c-name")
        .filter(|v| !v.is_empty())
        .cloned()
        .unwrap_or_else(|| sender_ip.to_string())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn decode_xml_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

fn percent_decode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        let c = input[i..].chars().next().unwrap_or(' ');
        match c {
            '+' => {
                out.push(' ');
                i += 1;
            }
            '%' if i + 2 < input.len() => {
                let hex = &input[i + 1..i + 3];
                if let Ok(code) = u8::from_str_radix(hex, 16) {
                    out.push(code as char);
                    i += 3;
                } else {
                    out.push(c);
                    i += 1;
                }
            }
            _ => {
                out.push(c);
                i += c.len_utf8();
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_soap_action_parses_quoted_header() {
        let (action, params) = parse_soap_action(
            "\"urn:schemas-upnp-org:service:AVTransport:1#SetAVTransportURI\"",
            r#"<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body><u:SetAVTransportURI xmlns:u="urn:schemas-upnp-org:service:AVTransport:1"><InstanceID>0</InstanceID><CurrentURI>http://x/y.mp4</CurrentURI><CurrentURIMetaData><DIDL-Lite/></CurrentURIMetaData></u:SetAVTransportURI></s:Body></s:Envelope>"#,
        );
        assert_eq!(action, "SetAVTransportURI");
        assert_eq!(
            params.get("CurrentURI").map(String::as_str),
            Some("http://x/y.mp4")
        );
        assert_eq!(params.get("InstanceID").map(String::as_str), Some("0"));
    }

    #[test]
    fn parse_dlna_time_handles_hh_mm_ss() {
        assert_eq!(parse_dlna_time_to_ms("00:01:02"), 62_000);
        assert_eq!(parse_dlna_time_to_ms("01:02:03.500"), 3_723_000);
        assert_eq!(parse_dlna_time_to_ms("bad"), -1);
    }

    #[test]
    fn clean_title_strips_extension_and_decodes() {
        assert_eq!(clean_media_title("song%20name.mp3"), "song name");
        assert_eq!(clean_media_title("movie.mp4"), "movie");
        assert_eq!(clean_media_title("no ext"), "no ext");
    }

    #[test]
    fn extract_media_type_from_meta_tiers() {
        let meta_with_pi = r#"<DIDL-Lite><res protocolInfo="http-get:*:video/mp4:*">http://x/a</res><upnp:class>object.item.videoItem</upnp:class></DIDL-Lite>"#;
        assert_eq!(
            extract_media_type_from_didl_meta(meta_with_pi, ""),
            DlnaMediaType::VIDEO
        );
        let meta_with_class = r#"<upnp:class>object.item.audioItem.musicTrack</upnp:class>"#;
        assert_eq!(
            extract_media_type_from_didl_meta(meta_with_class, ""),
            DlnaMediaType::AUDIO
        );
        assert_eq!(
            extract_media_type_from_didl_meta("", "http://x/song.mp3"),
            DlnaMediaType::AUDIO
        );
        assert_eq!(
            extract_media_type_from_didl_meta("", "http://x/photo.jpg"),
            DlnaMediaType::IMAGE
        );
    }

    #[test]
    fn album_art_extraction() {
        let meta = "<upnp:albumArtURI>http://x/art.jpg</upnp:albumArtURI>";
        assert_eq!(
            extract_album_art_uri_from_didl_meta(meta),
            "http://x/art.jpg"
        );
        assert_eq!(extract_album_art_uri_from_didl_meta("no art"), "");
    }
    #[test]
    fn protocol_mime_wins_over_conflicting_class_and_url() {
        for (mime, expected) in [
            ("video/mp4", DlnaMediaType::VIDEO),
            ("video/mp2t", DlnaMediaType::VIDEO),
            ("audio/flac", DlnaMediaType::AUDIO),
            ("image/png", DlnaMediaType::IMAGE),
        ] {
            let meta = format!(
                r#"<item><upnp:class>object.item.audioItem.musicTrack</upnp:class><res protocolInfo="http-get:*:{mime}:*">http://host/fs?id=a</res></item>"#
            );
            assert_eq!(
                extract_media_type_from_didl_meta(&meta, "http://host/a.mp4"),
                expected
            );
        }
        for url in ["", "http://host/fs?id=unknown", "http://host/a.unknownext"] {
            assert_eq!(
                extract_media_type_from_didl_meta("", url),
                DlnaMediaType::UNKNOWN
            );
        }
    }
}
