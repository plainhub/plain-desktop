//! Unit tests for `src/fsx.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;
use std::path::Path;
use std::path::PathBuf;

#[test]
fn decodes_plain_ascii() {
    assert_eq!(percent_decode_path("hello"), "hello");
}

/// plain-app media-item envelope ids decrypt to `{"path":…,"mediaId":…}`;
/// the resolver must return the path, not the JSON document (raw-path
/// ids and non-JSON plaintext stay untouched).
#[test]
fn extract_path_accepts_media_item_envelope() {
    assert_eq!(
        extract_path(r#"{"path":"/x/b.svg","mediaId":"87772750-b50d-5f1b"}"#),
        "/x/b.svg"
    );
    assert_eq!(extract_path("/x/b.svg"), "/x/b.svg");
    assert_eq!(extract_path("/x/plain file.svg"), "/x/plain file.svg");
    // A JSON doc without a string "path" is not an envelope: pass through
    // (the fs handler will 404 on stat, same as any bogus path).
    assert_eq!(
        extract_path(r#"{"file":"/x/b.svg"}"#),
        r#"{"file":"/x/b.svg"}"#
    );
}

#[test]
fn path_from_file_id_roundtrips_both_shapes() {
    let dir = tempfile::tempdir().unwrap();
    let prefs = crate::prefs::Prefs::load(&dir.path().join("prefs.json")).unwrap();
    let token = "dG9rZW5fdG9rZW5fdG9rZW5fdG9rZW5fdG9rZW5fdG8="; // 32 bytes
    prefs.set("url_token", token).unwrap();
    let key = base64_decode(token).unwrap();

    let raw = crate::crypto::xchacha_encrypt_raw(&key, b"/x/a.jpg").unwrap();
    let id_raw = crate::utils::base64::base64_encode(&raw);
    assert_eq!(
        path_from_file_id(&id_raw, &prefs).unwrap(),
        "/x/a.jpg",
        "bare-path ids keep working"
    );

    let envelope = format!(
        r#"{{"path":"/home/smartbox/memory/plainapp/design/b.svg","mediaId":"87772750-b50d-5f1b-912e-19614bf7d366"}}"#
    );
    let enc = crate::crypto::xchacha_encrypt_raw(&key, envelope.as_bytes()).unwrap();
    let id_env = crate::utils::base64::base64_encode(&enc);
    assert_eq!(
        path_from_file_id(&id_env, &prefs).unwrap(),
        "/home/smartbox/memory/plainapp/design/b.svg",
        "plain-app media-item envelope ids resolve to their path"
    );
}

#[test]
fn decodes_percent_encoded_ascii() {
    assert_eq!(percent_decode_path("hello%20world"), "hello world");
    assert_eq!(percent_decode_path("a%2Bb"), "a+b");
}

#[test]
fn decodes_utf8_multibyte() {
    // 中 = E4 B8 AD
    assert_eq!(percent_decode_path("%E4%B8%AD"), "中");
}

#[test]
fn invalid_percent_passes_through() {
    // '%XY' with X not a hex digit → pass the '%' through and let the
    // next iteration handle the rest. Matches lossless upstream.
    assert_eq!(percent_decode_path("%G0"), "%G0");
}

#[test]
fn trailing_percent_does_not_panic() {
    // '%' at the end (no following two chars) → pass through.
    assert_eq!(percent_decode_path("foo%"), "foo%");
}

#[test]
fn mime_known_extensions() {
    assert_eq!(guess_mime(Path::new("a.mp4")), "video/mp4");
    assert_eq!(guess_mime(Path::new("a.PNG")), "image/png");
    assert_eq!(guess_mime(Path::new("a.html")), "text/html; charset=utf-8");
    assert_eq!(guess_mime(Path::new("a.json")), "application/json");
    assert_eq!(
        guess_mime(Path::new("a.md")),
        "text/markdown; charset=utf-8"
    );
    assert_eq!(guess_mime(Path::new("a.csv")), "text/csv; charset=utf-8");
    assert_eq!(
        guess_mime(Path::new("a.tsv")),
        "text/tab-separated-values; charset=utf-8"
    );
}

#[test]
fn mime_unknown_falls_back_to_octet_stream() {
    assert_eq!(guess_mime(Path::new("a.zzz")), "application/octet-stream");
    assert_eq!(guess_mime(Path::new("a")), "application/octet-stream");
}

fn write_temp_file(name: &str, bytes: &[u8]) -> PathBuf {
    let dir = std::env::temp_dir().join("plainnas-fsx-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, bytes).unwrap();
    p
}

#[test]
fn animated_or_svg_by_extension_and_header() {
    // .svg always passes; png/jpg/jpeg always fail regardless of content.
    assert!(is_animated_image_or_svg(Path::new("x/logo.svg")));
    assert!(!is_animated_image_or_svg(Path::new("x/a.png")));
    assert!(!is_animated_image_or_svg(Path::new("x/a.jpg")));
    // Non-photo extensions never pass, even with a `<svg` payload.
    let p = write_temp_file("note.txt", b"<svg xmlns='x'/>");
    assert!(!is_animated_image_or_svg(&p));

    // GIF magic → true.
    let p = write_temp_file("a.gif", b"GIF89a\0\0\0\0");
    assert!(is_animated_image_or_svg(&p));

    // Real static WebP (VP8X without the animation bit) → false.
    let mut webp = b"RIFF\0\0\0\0WEBPVP8X".to_vec();
    webp.extend_from_slice(&[0; 10]);
    let p = write_temp_file("static.webp", &webp);
    assert!(!is_animated_image_or_svg(&p));

    // Animated WebP: VP8X box with the animation bit (byte 16, bit 1).
    let mut webp_anim = b"RIFF\0\0\0\0WEBPVP8X".to_vec();
    webp_anim.extend_from_slice(&[0b10, 0, 0, 0]);
    webp_anim.extend_from_slice(&[0; 6]);
    let p = write_temp_file("anim.webp", &webp_anim);
    assert!(is_animated_image_or_svg(&p));

    // HEIF brands: msf1 (animated) → true, heic (still) → false.
    let mut heif_anim = b"\0\0\0\0ftypmsf1".to_vec();
    heif_anim.extend_from_slice(&[0; 8]);
    let p = write_temp_file("anim.heic", &heif_anim);
    assert!(is_animated_image_or_svg(&p));
    let mut heif_still = b"\0\0\0\0ftypheic".to_vec();
    heif_still.extend_from_slice(&[0; 8]);
    let p = write_temp_file("still.heic", &heif_still);
    assert!(!is_animated_image_or_svg(&p));

    // SVG sniffed from content when the extension lies.
    let p = write_temp_file(
        "vector.avif",
        b"<?xml version=\"1.0\"?><svg xmlns=\"\"></svg>",
    );
    assert!(is_animated_image_or_svg(&p));
}

#[test]
fn directory_counts_work_on_host_platform() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("visible.txt"), b"a").unwrap();
    std::fs::write(dir.path().join(".hidden.txt"), b"b").unwrap();
    assert_eq!(count_dir_entries(dir.path(), false).unwrap(), 1);
    assert_eq!(count_dir_entries(dir.path(), true).unwrap(), 2);
    assert_eq!(count_dir_entries_fast(dir.path(), 10).unwrap(), 2);
}
