use super::*;

#[derive(serde::Deserialize)]
pub(super) struct FileQuery {
    id: String,
}
pub(super) async fn file(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Query(query): Query<FileQuery>,
) -> axum::response::Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(decoded) = crate::xchacha_decrypt(
        &crate::prefs::ensure_url_token(&state.prefs),
        &crate::base64_decode(&query.id),
    ) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(uri) = String::from_utf8(decoded) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let path = Path::new(&uri);
    let Some(id) = path.file_stem().and_then(|v| v.to_str()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(record) = state.db.get_app_file(id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let store = crate::app_files::FileStore::new(state.db.clone(), state.directory.clone());
    let Some(suffix) = Path::new(&record.real_path)
        .file_name()
        .and_then(|s| s.to_str())
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(owned) = store.resolve(suffix) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if path != owned {
        return StatusCode::NOT_FOUND.into_response();
    }
    stream_file(&owned, &record.mime_type, &headers).await
}

async fn stream_file(path: &Path, mime: &str, headers: &HeaderMap) -> axum::response::Response {
    use crate::utils::http::RangeParse;
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    let Ok(mut file) = tokio::fs::File::open(path).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(metadata) = file.metadata().await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !metadata.is_file() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let size = metadata.len();
    let range = headers
        .get(axum::http::header::RANGE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let (status, start, length, end) = match crate::utils::http::parse_range_header(range, size) {
        RangeParse::Full => (StatusCode::OK, 0, size, None),
        RangeParse::Partial(start, end) => (
            StatusCode::PARTIAL_CONTENT,
            start,
            end - start + 1,
            Some(end),
        ),
        RangeParse::Unsatisfiable => {
            return (
                StatusCode::RANGE_NOT_SATISFIABLE,
                [(axum::http::header::CONTENT_RANGE, format!("bytes */{size}"))],
            )
                .into_response();
        }
    };
    if file.seek(std::io::SeekFrom::Start(start)).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let stream =
        futures_util::stream::try_unfold((file, length), |(mut file, remaining)| async move {
            if remaining == 0 {
                return Ok::<_, std::io::Error>(None);
            }
            let mut buffer = vec![0; remaining.min(64 * 1024) as usize];
            let read = file.read(&mut buffer).await?;
            if read == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "app file truncated",
                ));
            }
            buffer.truncate(read);
            Ok(Some((buffer, (file, remaining - read as u64))))
        });
    let mut response = axum::body::Body::from_stream(stream).into_response();
    *response.status_mut() = status;
    let output = response.headers_mut();
    output.insert(
        axum::http::header::CONTENT_LENGTH,
        length.to_string().parse().unwrap(),
    );
    output.insert(axum::http::header::ACCEPT_RANGES, "bytes".parse().unwrap());
    output.insert(
        axum::http::header::CACHE_CONTROL,
        "private, max-age=3600".parse().unwrap(),
    );
    if let Ok(value) = mime.parse() {
        output.insert(axum::http::header::CONTENT_TYPE, value);
    }
    if let Some(end) = end {
        output.insert(
            axum::http::header::CONTENT_RANGE,
            format!("bytes {start}-{end}/{size}").parse().unwrap(),
        );
    }
    response
}
