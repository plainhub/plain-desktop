use std::io::SeekFrom;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWrite, AsyncWriteExt};

use super::super::graphql::context::AppCtx;
use super::response::respond;
use super::uri::{parse_decrypted_id, resolve_uri};
use plain_rs::xchacha_decrypt;
use plain_rs::base64_decode;
use plain_rs::mime::mime_from_ext;
use plain_rs::query::parse_query;

/// Serve a file via the local server's `/fs` endpoint.
///
/// Mirrors `plain-app` `web/routes/FilesRoutes.kt::addFilesRoutes().get("/fs")`:
///   1. URL-decode the `id` query param.
///   2. Base64-decode + XChaCha20-decrypt with the local server's URL
///      token (this is how the web client delivers the path — see
///      `getFileId` in `lib/api/file.ts`).
///   3. Parse the decrypted payload: either a JSON object
///      `{"path":"…","mediaId":"…","name":"…"}` or a plain URI string
///      such as `fid:{sha256}.{ext}` / `app://…` / absolute path.
///   4. Resolve to a real on-disk path. For `fid:` the resolution is
///      `{data_dir}/files/{aa}/{bb}/{hash}.{ext}` — matches what
///      `app_file_store::import_file` writes.
///   5. Byte-range short-circuit (`?offset=…&length=…`) for BLE
///      transports — serves raw `application/octet-stream` bytes.
///   6. Otherwise stream the file body with RFC 5987
///      `Content-Disposition`, honoring HTTP `Range` headers (RFC 7233)
///      so browsers can seek media.
pub(super) async fn serve_file<W: AsyncWrite + Unpin>(
    wr: &mut W,
    query_str: &str,
    range_header: &str,
    ctx: &Arc<AppCtx>,
) {
    // 1. Parse query params.
    let params = parse_query(query_str);
    let id_encoded = match params.get("id") {
        Some(s) if !s.is_empty() => s.clone(),
        _ => {
            respond(wr, 400, "Bad Request", b"missing id", "text/plain").await;
            return;
        }
    };

    // 2. Decrypt the id.
    let id_bytes = base64_decode(&id_encoded);
    let Some(plaintext) = xchacha_decrypt(&ctx.token, &id_bytes) else {
        respond(wr, 401, "Unauthorized", b"", "text/plain").await;
        return;
    };
    let plaintext = match std::str::from_utf8(&plaintext) {
        Ok(s) => s.to_string(),
        Err(_) => {
            respond(
                &mut *wr,
                400,
                "Bad Request",
                b"decrypted id is not valid utf-8",
                "text/plain",
            )
            .await;
            return;
        }
    };

    // 3. Parse the decrypted payload.
    let (path, json_name) = parse_decrypted_id(&plaintext);

    // 4. Resolve to a real path.
    let resolved = resolve_uri(&path, &ctx.data_dir);

    // 5. Sanity-check the file is on disk and is a file.
    let metadata = match tokio::fs::metadata(&resolved).await {
        Ok(m) => m,
        Err(_) => {
            respond(wr, 404, "Not Found", b"", "text/plain").await;
            return;
        }
    };
    if !metadata.is_file() {
        respond(wr, 400, "Bad Request", b"not a file", "text/plain").await;
        return;
    }
    let file_size = metadata.len();

    // 6. BLE byte-range request: `?offset=…&length=…`. Mirrors plain-app
    //    `FilesRoutes.kt`'s `readFileRange(path, rangeOffset, rangeLength)`
    //    branch — used by low-throughput transports (BLE) to download a
    //    file in small chunks. Only applies when `length > 0`; serves raw
    //    `application/octet-stream` bytes with no Content-Disposition,
    //    no thumbnails, no conversion. A request past EOF responds 404
    //    (matching Android's `readFileRange == null` path).
    if let (Some(off), Some(len)) = (
        params.get("offset").and_then(|s| s.parse::<u64>().ok()),
        params.get("length").and_then(|s| s.parse::<u64>().ok()),
    )
        && len > 0 {
            if off >= file_size {
                respond(wr, 404, "Not Found", b"", "text/plain").await;
                return;
            }
            let clamped = len.min(file_size - off);
            serve_range_raw(wr, &resolved, off, clamped).await;
            return;
        }

    // 7. Display filename + MIME + Content-Disposition (RFC 5987).
    //    plain-app URL-encodes the filename for both the legacy
    //    `filename="…"` and the `filename*=utf-8''…` forms — we match
    //    that exactly so non-ASCII names round-trip correctly.
    let display_name = if !json_name.is_empty() {
        json_name
    } else {
        resolved
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file")
            .to_string()
    };
    let mime = mime_from_ext(&display_name);
    let is_download = params.get("dl").map(|s| s.as_str()) == Some("1");
    let disposition_kind = if is_download { "attachment" } else { "inline" };
    let disposition =
        plain_rs::utils::http::content_disposition(disposition_kind, &display_name);

    // 8. HTTP `Range` header (RFC 7233). Browsers use this for media
    //    seeking; plain-app gets it implicitly via Ktor's `respondFile`,
    //    the local server has to handle it explicitly. Only single-range
    //    requests are honored; multi-range falls through to a full 200.
    if !range_header.is_empty()
        && let Some((start, end)) =
            plain_rs::utils::http::parse_range_header(range_header, file_size)
    {
        serve_partial(wr, &resolved, start, end, file_size, mime, &disposition).await;
        return;
    }

    // 9. Full response (200) with `accept-ranges: bytes` so clients know
    //    they can issue `Range` requests on subsequent calls.
    let head = format!(
        "HTTP/1.1 200 OK\r\n\
         content-type: {mime}\r\n\
         content-length: {file_size}\r\n\
         content-disposition: {disposition}\r\n\
         accept-ranges: bytes\r\n\
         access-control-expose-headers: content-disposition, accept-ranges, content-range\r\n\
         access-control-allow-origin: *\r\n\
         access-control-allow-methods: GET, POST, PUT, DELETE, OPTIONS\r\n\
         access-control-allow-headers: *\r\n\
         connection: close\r\n\
         \r\n"
    );
    if wr.write_all(head.as_bytes()).await.is_err() {
        return;
    }
    stream_file(wr, &resolved, 0, file_size).await;
}

/// Serve a raw byte range for BLE transport. Content-Type is
/// `application/octet-stream` (matches plain-app), with no
/// Content-Disposition and no Range negotiation — the caller has
/// already validated `offset` / `length`.
async fn serve_range_raw<W: AsyncWrite + Unpin>(
    wr: &mut W,
    path: &std::path::Path,
    offset: u64,
    length: u64,
) {
    let head = format!(
        "HTTP/1.1 200 OK\r\n\
         content-type: application/octet-stream\r\n\
         content-length: {length}\r\n\
         access-control-allow-origin: *\r\n\
         access-control-allow-methods: GET, POST, PUT, DELETE, OPTIONS\r\n\
         access-control-allow-headers: *\r\n\
         connection: close\r\n\
         \r\n"
    );
    if wr.write_all(head.as_bytes()).await.is_err() {
        return;
    }
    stream_file(wr, path, offset, length).await;
}

/// Serve a `206 Partial Content` response for an HTTP `Range` request.
async fn serve_partial<W: AsyncWrite + Unpin>(
    wr: &mut W,
    path: &std::path::Path,
    start: u64,
    end: u64,
    file_size: u64,
    mime: &str,
    disposition: &str,
) {
    let length = end - start + 1;
    let head = format!(
        "HTTP/1.1 206 Partial Content\r\n\
         content-type: {mime}\r\n\
         content-length: {length}\r\n\
         content-range: bytes {start}-{end}/{file_size}\r\n\
         content-disposition: {disposition}\r\n\
         accept-ranges: bytes\r\n\
         access-control-expose-headers: content-disposition, accept-ranges, content-range\r\n\
         access-control-allow-origin: *\r\n\
         access-control-allow-methods: GET, POST, PUT, DELETE, OPTIONS\r\n\
         access-control-allow-headers: *\r\n\
         connection: close\r\n\
         \r\n"
    );
    if wr.write_all(head.as_bytes()).await.is_err() {
        return;
    }
    stream_file(wr, path, start, length).await;
}

/// Stream `length` bytes from `path` starting at `offset`, in 64 KB
/// chunks. The header must already have been written by the caller.
/// Errors after the header is sent are silently dropped — the
/// `connection: close` framing means the client will see a truncated
/// body and retry.
async fn stream_file<W: AsyncWrite + Unpin>(
    wr: &mut W,
    path: &std::path::Path,
    offset: u64,
    length: u64,
) {
    let mut file = match tokio::fs::File::open(path).await {
        Ok(f) => f,
        Err(_) => return,
    };
    if offset > 0 && file.seek(SeekFrom::Start(offset)).await.is_err() {
        return;
    }
    let mut remaining = length as usize;
    let mut buf = vec![0u8; 64 * 1024];
    while remaining > 0 {
        let to_read = remaining.min(buf.len());
        let n = match file.read(&mut buf[..to_read]).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        if wr.write_all(&buf[..n]).await.is_err() {
            break;
        }
        remaining -= n;
    }
    let _ = wr.flush().await;
}


#[cfg(test)]
mod tests {
    use super::*;







}
