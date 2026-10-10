use super::server::ServerState;
use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn query(request: &axum::http::request::Parts, key: &str) -> Option<String> {
    crate::utils::query::query_get(&request.uri.to_string(), key)
}
async fn primitive(
    state: &ServerState,
    operation: &str,
    path: &str,
    extra: Value,
) -> anyhow::Result<Value> {
    state
        .host
        .call_wait(
            "systemFileResource",
            json!({"operation":operation,"path":path,"params":extra}),
        )
        .await
        .map_err(anyhow::Error::msg)
}
pub(super) async fn resolve(state: &ServerState, path: &str) -> anyhow::Result<String> {
    resolve_reference(&state.host, state.db.clone(), state.directory.clone(), path).await
}
pub(super) async fn resolve_reference(
    host: &std::sync::Arc<super::host::Host>,
    db: std::sync::Arc<crate::db::Db>,
    directory: PathBuf,
    path: &str,
) -> anyhow::Result<String> {
    if path
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("fid:"))
    {
        return Ok(crate::app_files::FileStore::new(db, directory)
            .resolve(&path[4..])
            .map_err(anyhow::Error::msg)?
            .to_string_lossy()
            .into_owned());
    }
    if path
        .get(..6)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("app://"))
    {
        let dir = host
            .call(
                "systemFileResource",
                json!({"operation":"appDir","path":"","params":{}}),
            )
            .await
            .map_err(anyhow::Error::msg)?;
        let suffix = Path::new(&path[6..]);
        anyhow::ensure!(
            !suffix.is_absolute()
                && !suffix
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir)),
            "Invalid app file path"
        );
        return Ok(Path::new(
            dir.as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing app directory"))?,
        )
        .join(suffix)
        .to_string_lossy()
        .into_owned());
    }
    Ok(path.to_owned())
}
pub(super) async fn file(State(state): State<ServerState>, request: Request) -> Response {
    let request = request.into_parts().0;
    let id = query(&request, "id").unwrap_or_default();
    if id.is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let resolved = async {
        if let Some(sid) = query(&request, "sid") {
            let shares = crate::shares::Service::new(state.db.clone(), state.prefs.clone());
            let path = shares
                .resolve_file(&sid, &id)?
                .ok_or_else(|| anyhow::anyhow!("Shared file unavailable"))?;
            return Ok((
                path.clone(),
                String::new(),
                path.rsplit('/').next().unwrap_or_default().to_owned(),
            ));
        }
        let plain = crate::xchacha_decrypt(
            &crate::prefs::ensure_url_token(&state.prefs),
            &crate::base64_decode(&id),
        )
        .ok_or_else(|| anyhow::anyhow!("Invalid file id"))?;
        let plain = resolve(&state, &String::from_utf8(plain)?).await?;
        if plain.starts_with('{') {
            let value: Value = serde_json::from_str(&plain)?;
            let path = resolve(&state, value["path"].as_str().unwrap_or_default()).await?;
            Ok::<_, anyhow::Error>((
                path,
                value["mediaId"].as_str().unwrap_or_default().to_owned(),
                value["name"].as_str().unwrap_or_default().to_owned(),
            ))
        } else {
            Ok((plain, String::new(), String::new()))
        }
    }
    .await;
    match resolved {
        Ok((path, media, name)) => match serve(&state, &request, &path, &media, &name).await {
            Ok(response) => response,
            Err(error) => (
                StatusCode::FORBIDDEN,
                format!("File is expired or does not exist. {error}"),
            )
                .into_response(),
        },
        Err(error) => (
            StatusCode::FORBIDDEN,
            format!("File is expired or does not exist. {error}"),
        )
            .into_response(),
    }
}
pub(super) fn escape(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (*b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}
fn disposition(name: &str, download: bool) -> String {
    let name = escape(name);
    format!(
        "{}; filename=\"{name}\"; filename*=utf-8''{name}",
        if download { "attachment" } else { "inline" }
    )
}
pub(super) fn secure(mut response: Response, name: Option<&str>, download: bool) -> Response {
    response
        .headers_mut()
        .insert("x-content-type-options", "nosniff".parse().unwrap());
    let mime = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default();
    if mime == "text/html" || mime.ends_with("+xml") || mime.ends_with("/xml") {
        response
            .headers_mut()
            .insert("content-security-policy", "sandbox".parse().unwrap());
    }
    if let Some(name) = name {
        if let Ok(value) = disposition(name, download).parse() {
            response
                .headers_mut()
                .insert(header::CONTENT_DISPOSITION, value);
        }
        response.headers_mut().insert(
            "access-control-expose-headers",
            "Content-Disposition".parse().unwrap(),
        );
    }
    response
}
struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
async fn converted(
    state: &ServerState,
    operation: &str,
    path: &str,
    mime: &str,
    headers: &HeaderMap,
) -> anyhow::Result<Option<Response>> {
    let output = state
        .directory
        .join(format!(".resource_{}", uuid::Uuid::new_v4()));
    let temporary = Temporary(output.clone());
    let value = primitive(state, operation, path, json!({"output":output})).await?;
    if value.as_bool() != Some(true) {
        return Ok(None);
    }
    let response = super::server::files::stream_file(&output, mime, headers).await;
    let (parts, body) = response.into_parts();
    let stream = body.into_data_stream().map(move |chunk| {
        let _ = &temporary;
        chunk
    });
    Ok(Some(Response::from_parts(parts, Body::from_stream(stream))))
}
pub(super) async fn serve(
    state: &ServerState,
    request: &axum::http::request::Parts,
    path: &str,
    media_id: &str,
    name: &str,
) -> anyhow::Result<Response> {
    let download = query(request, "dl").as_deref() == Some("1");
    if path.contains("!zip!/") {
        let cached = primitive(state, "zipExtract", path, json!({})).await?;
        let Some(cached) = cached.as_str() else {
            return Ok(StatusCode::NOT_FOUND.into_response());
        };
        let filename = if name.is_empty() {
            path.trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or_default()
        } else {
            name
        };
        let mime = crate::utils::mime::mime_from_ext(cached);
        let response =
            super::server::files::stream_file(Path::new(cached), mime, &request.headers).await;
        return Ok(secure(response, Some(filename), download));
    }
    if let (Some(offset), Some(length)) = (
        query(request, "offset").and_then(|v| v.parse::<i64>().ok()),
        query(request, "length").and_then(|v| v.parse::<usize>().ok()),
    ) {
        if length > 0 && !path.starts_with("content://") && !path.starts_with("pkgicon://") {
            if length > 256 * 1024 {
                return Ok((StatusCode::BAD_REQUEST, "range length is too large").into_response());
            }
            use tokio::io::{AsyncReadExt, AsyncSeekExt};
            if offset < 0 {
                return Ok(StatusCode::NOT_FOUND.into_response());
            }
            let Ok(mut file) = tokio::fs::File::open(path).await else {
                return Ok(StatusCode::NOT_FOUND.into_response());
            };
            file.seek(std::io::SeekFrom::Start(offset as u64)).await?;
            let mut bytes = vec![0; length];
            let mut n = 0;
            while n < length {
                let count = file.read(&mut bytes[n..]).await?;
                if count == 0 {
                    break;
                }
                n += count;
            }
            bytes.truncate(n);
            return Ok(secure(
                ([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response(),
                None,
                false,
            ));
        }
    }
    if path.starts_with("content://") {
        if let Some(response) =
            converted(state, "convert3gp", path, "video/mp4", &request.headers).await?
        {
            return Ok(secure(response, None, false));
        }
        #[cfg(feature = "http_transport")]
        {
            let body = state
                .bridge
                .resource(path, state.stop.clone())
                .await
                .map_err(|s| anyhow::anyhow!("Resource stream failed: {s}"))?;
            let response =
                ([(header::CONTENT_TYPE, "application/octet-stream")], body).into_response();
            let filename = if name.is_empty() {
                path.rsplit('/').next().unwrap_or_default()
            } else {
                name
            };
            return Ok(secure(response, download.then_some(filename), download));
        }
        #[cfg(not(feature = "http_transport"))]
        {
            return Ok(StatusCode::NOT_FOUND.into_response());
        }
    }
    if let Some(package) = path.strip_prefix("pkgicon://") {
        return Ok(
            match converted(
                state,
                "packageIcon",
                package,
                "application/octet-stream",
                &request.headers,
            )
            .await?
            {
                Some(response) => secure(response, None, false),
                None => StatusCode::NOT_FOUND.into_response(),
            },
        );
    }
    let meta = match tokio::fs::metadata(path).await {
        Ok(meta) => meta,
        Err(_) => return Ok(StatusCode::NOT_FOUND.into_response()),
    };
    if meta.is_dir() {
        return Ok(StatusCode::BAD_REQUEST.into_response());
    }
    let filename = if name.is_empty() {
        path.rsplit('/').next().unwrap_or_default()
    } else {
        name
    };
    let mime = crate::utils::mime::mime_from_ext(path);
    if query(request, "probe").as_deref() == Some("1") {
        let codec = primitive(state, "probe", path, json!({})).await?;
        let codec: String = codec
            .as_str()
            .unwrap_or_default()
            .chars()
            .filter(|ch| ch.is_alphanumeric())
            .collect();
        return Ok(secure(
            axum::Json(json!({"codec":codec})).into_response(),
            None,
            false,
        ));
    }
    if !download {
        let animated = if mime.starts_with("image/") {
            primitive(
                state,
                "animated",
                path,
                json!({"fileName":escape(filename)}),
            )
            .await?
            .as_bool()
                == Some(true)
        } else {
            false
        };
        if !animated {
            if let (Some(width), Some(height)) = (
                query(request, "w").and_then(|v| v.parse::<u32>().ok()),
                query(request, "h").and_then(|v| v.parse::<u32>().ok()),
            ) {
                let response = super::thumbnails::response(
                    state,
                    super::thumbnails::Request {
                        path: path.to_owned(),
                        width,
                        height,
                        center_crop: query(request, "cc").as_deref() != Some("false"),
                        media_id: media_id.to_owned(),
                        file_name: escape(filename),
                        if_none_match: request
                            .headers
                            .get(header::IF_NONE_MATCH)
                            .and_then(|v| v.to_str().ok())
                            .map(str::to_owned),
                    },
                )
                .await;
                return Ok(secure(response, Some(filename), false));
            }
            if let Some(response) =
                converted(state, "decodePng", path, "image/png", &request.headers).await?
            {
                return Ok(secure(response, Some(filename), false));
            }
            if mime == "video/mp4" {
                if query(request, "tr").as_deref() == Some("1") {
                    let codec = primitive(state, "probe", path, json!({})).await?;
                    if matches!(codec.as_str(), Some("hvc1" | "hev1")) {
                        let output = primitive(state, "transcode", path, json!({}))
                            .await
                            .unwrap_or(Value::Null);
                        return Ok(match output.as_str() {
                            Some(output) => secure(
                                super::server::files::stream_file(
                                    Path::new(output),
                                    mime,
                                    &request.headers,
                                )
                                .await,
                                Some(filename),
                                false,
                            ),
                            None => (
                                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                                "video transcoding is not available for this file",
                            )
                                .into_response(),
                        });
                    }
                }
                if let Some(output) = primitive(state, "remux", path, json!({})).await?.as_str() {
                    return Ok(secure(
                        super::server::files::stream_file(
                            Path::new(output),
                            mime,
                            &request.headers,
                        )
                        .await,
                        Some(filename),
                        false,
                    ));
                }
            }
        }
    }
    Ok(secure(
        super::server::files::stream_file(Path::new(path), mime, &request.headers).await,
        Some(filename),
        download,
    ))
}
