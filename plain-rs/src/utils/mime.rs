/// Best-effort MIME type guess from a filename extension. Returns
/// `application/octet-stream` for unknown / extensionless inputs.
///
/// Merged table (2026-09-04): union of the former plain-rs table and the
/// richer plain-nas `fsx::guess_mime` table (which mirrored the Go NAS),
/// plus plain-app conventions where they overlapped. Charset handling is
/// deliberately excluded — callers append `; charset=utf-8` for `text/*`
/// if their transport wants it.
pub fn mime_from_ext(filename: &str) -> &'static str {
    match filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        // Text
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" => "application/javascript",
        "json" => "application/json",
        "xml" => "application/xml",
        "txt" | "log" => "text/plain",
        "csv" => "text/csv",
        "tsv" => "text/tab-separated-values",
        "md" => "text/markdown",
        // Images
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "heic" | "heif" => "image/heic",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        // Videos
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "3gp" => "video/3gpp",
        "avi" => "video/x-msvideo",
        "ogv" => "video/ogg",
        "ts" => "video/mp2t",
        // Audio
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "wav" => "audio/wav",
        "ogg" | "oga" => "audio/ogg",
        "flac" => "audio/flac",
        "opus" => "audio/opus",
        "aac" => "audio/aac",
        // Fonts
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        // Archives
        "zip" => "application/zip",
        "tar" => "application/x-tar",
        "gz" | "tgz" => "application/gzip",
        "bz2" => "application/x-bzip2",
        "7z" => "application/x-7z-compressed",
        "rar" => "application/vnd.rar",
        "xz" => "application/x-xz",
        // Documents
        "pdf" => "application/pdf",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        // Default
        _ => "application/octet-stream",
    }
}

/// Reverse of `mime_from_ext`: pick a reasonable file extension for a
/// given MIME type. Returns `"bin"` for unknown / opaque types — the
/// caller can decide whether to surface a generic name. Matches
/// `plain-app` `AppFileStore.extFromMime` (Android `MimeTypeMap`), which
/// is why the image aliases (e.g. `image/x-icon` → `ico`) are included
/// here — favicon fetches use that MIME and must not lose the extension.
pub fn mime_extension(mime: &str) -> &'static str {
    match mime.to_ascii_lowercase().as_str() {
        // Images
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "image/bmp" => "bmp",
        "image/tiff" => "tif",
        "image/heic" => "heic",
        "image/avif" => "avif",
        "image/x-icon" | "image/vnd.microsoft.icon" => "ico",
        // Videos
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "video/quicktime" => "mov",
        "video/x-matroska" => "mkv",
        "video/3gpp" | "video/3gpp2" => "3gp",
        "video/x-msvideo" => "avi",
        "video/ogg" => "ogv",
        "video/mp2t" => "ts",
        // Audio
        "audio/mpeg" => "mp3",
        "audio/mp4" => "m4a",
        "audio/wav" | "audio/x-wav" => "wav",
        "audio/ogg" => "ogg",
        "audio/flac" => "flac",
        "audio/opus" => "opus",
        "audio/aac" => "aac",
        // Documents / data
        "application/pdf" => "pdf",
        "application/msword" => "doc",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => "docx",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => "xlsx",
        "application/zip" | "application/x-zip-compressed" => "zip",
        "application/json" => "json",
        "application/xml" | "text/xml" => "xml",
        "application/x-rar-compressed" | "application/vnd.rar" => "rar",
        "application/x-7z-compressed" => "7z",
        "application/x-tar" => "tar",
        "application/gzip" | "application/x-gzip" => "gz",
        "application/x-bzip2" => "bz2",
        "application/x-xz" => "xz",
        "application/vnd.android.package-archive" => "apk",
        // Text
        "text/plain" => "txt",
        "text/html" => "html",
        "text/css" => "css",
        "application/javascript" => "js",
        "text/csv" => "csv",
        "text/tab-separated-values" => "tsv",
        "text/markdown" => "md",
        // Fonts
        "font/ttf" | "application/x-font-ttf" => "ttf",
        "font/otf" => "otf",
        "font/woff" => "woff",
        "font/woff2" => "woff2",
        // Unknown / opaque
        _ => "bin",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_guesses() {
        assert_eq!(mime_from_ext("cat.jpg"), "image/jpeg");
        assert_eq!(mime_from_ext("cat.JPG"), "image/jpeg");
        assert_eq!(mime_from_ext("movie.mp4"), "video/mp4");
        assert_eq!(mime_from_ext("thing.unknown"), "application/octet-stream");
    }

    #[test]
    fn mime_handles_no_extension() {
        assert_eq!(mime_from_ext("README"), "application/octet-stream");
    }

    #[test]
    fn mime_handles_empty() {
        assert_eq!(mime_from_ext(""), "application/octet-stream");
    }

    #[test]
    fn ext_roundtrips_through_mime() {
        for ext in [
            "jpg", "png", "gif", "webp", "svg", "bmp", "tif", "heic", "avif", "mp4", "webm", "mov",
            "mkv", "3gp", "mp3", "m4a", "wav", "ogg", "pdf", "zip", "json", "txt", "html", "md",
            // from the plain-nas table merge
            "css", "js", "xml", "csv", "tsv", "ico", "avi", "ogv", "ts", "flac", "opus", "aac",
            "bz2", "xz",
        ] {
            assert_eq!(
                mime_extension(mime_from_ext(&format!("f.{ext}"))),
                ext,
                "roundtrip failed for {ext}"
            );
        }
    }

    #[test]
    fn ext_roundtrips_office_documents() {
        for ext in ["doc", "docx", "xlsx"] {
            assert_eq!(
                mime_extension(mime_from_ext(&format!("f.{ext}"))),
                ext,
                "roundtrip failed for {ext}"
            );
        }
    }

    #[test]
    fn ext_falls_back_to_bin() {
        assert_eq!(mime_extension("application/x-totally-made-up"), "bin");
        assert_eq!(mime_extension(""), "bin");
    }

    #[test]
    fn ext_covers_image_aliases() {
        assert_eq!(mime_extension("image/x-icon"), "ico");
        assert_eq!(mime_extension("image/vnd.microsoft.icon"), "ico");
        assert_eq!(mime_extension("video/3gpp2"), "3gp");
        assert_eq!(mime_extension("audio/x-wav"), "wav");
        assert_eq!(mime_extension("application/x-zip-compressed"), "zip");
        assert_eq!(mime_extension("application/x-gzip"), "gz");
        assert_eq!(mime_extension("application/x-font-ttf"), "ttf");
        assert_eq!(mime_extension("IMAGE/PNG"), "png"); // case-insensitive
    }

    #[test]
    fn office_documents_cover_plain_app_doc_mimes() {
        // plain-app `DocMediaStoreHelper.extraDocumentMimeTypes` — the doc
        // formats beyond `text/*` the media doc queries must recognize.
        assert_eq!(mime_from_ext("report.doc"), "application/msword");
        assert_eq!(
            mime_from_ext("report.DOCX"),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        );
        assert_eq!(
            mime_from_ext("sheet.xlsx"),
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        );
    }

    #[test]
    fn merged_table_covers_former_nas_entries() {
        // Spot-check the entries that came from plain-nas's table.
        assert_eq!(mime_from_ext("index.htm"), "text/html");
        assert_eq!(mime_from_ext("app.mjs"), "application/javascript");
        assert_eq!(mime_from_ext("data.tsv"), "text/tab-separated-values");
        assert_eq!(mime_from_ext("sys.log"), "text/plain");
        assert_eq!(mime_from_ext("favicon.ico"), "image/x-icon");
        assert_eq!(mime_from_ext("clip.m4v"), "video/mp4");
        assert_eq!(mime_from_ext("old.avi"), "video/x-msvideo");
        assert_eq!(mime_from_ext("song.flac"), "audio/flac");
        assert_eq!(mime_from_ext("voice.opus"), "audio/opus");
        assert_eq!(mime_from_ext("pkg.xz"), "application/x-xz");
        assert_eq!(mime_from_ext("backup.tar"), "application/x-tar");
    }
}
