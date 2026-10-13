use crate::assembly::server::ContentServer;
use serde_json::Value;
use std::{path::Path, sync::Arc, time::UNIX_EPOCH};

pub async fn file_resource(core: Arc<ContentServer>, params: Value) -> Result<Value, String> {
    let request: crate::host::file_resource::Request =
        serde_json::from_value(params.clone()).map_err(|error| error.to_string())?;
    if !request.params.is_object() {
        return Err("Invalid file resource parameters".into());
    }
    match request.operation.as_str() {
        "animated" => tokio::task::spawn_blocking(move || {
            plain_media::operations::fsx::is_animated_image_or_svg(Path::new(&request.path))
        })
        .await
        .map(Value::Bool)
        .map_err(|error| error.to_string()),
        "probe" => Ok(Value::String(
            plain_media::previews::videos::probe_video_codec(&request.path).await,
        )),
        "transcode" => plain_media::previews::videos::transcode_mp4_for_browser(&request.path)
            .await
            .map(|path| Value::String(path.to_string_lossy().into_owned()))
            .map_err(|error| error.to_string()),
        "zipExtract" => {
            let cache = core.data_directory().join("cache/zip-entries");
            tokio::task::spawn_blocking(move || {
                crate::services::files::zip_entries::extract(&cache, &request.path)
            })
            .await
            .map_err(|error| error.to_string())?
            .map(|path| Value::String(path.to_string_lossy().into_owned()))
        }
        "remux" => Ok(Value::String(request.path)),
        "decodePng" => {
            let mime = plain_rs::utils::mime::mime_from_ext(&request.path);
            if !mime.starts_with("image/")
                || matches!(
                    mime,
                    "image/jpeg" | "image/png" | "image/gif" | "image/webp" | "image/svg+xml"
                )
            {
                return Ok(Value::Bool(false));
            }
            let output = request.params["output"]
                .as_str()
                .ok_or("PNG output path required")?
                .to_owned();
            tokio::task::spawn_blocking(move || -> Result<Value, String> {
                let image = match image::open(&request.path) {
                    Ok(image) => image,
                    Err(image::ImageError::Unsupported(_)) => return Ok(Value::Bool(false)),
                    Err(error) => return Err(error.to_string()),
                };
                image
                    .save_with_format(output, image::ImageFormat::Png)
                    .map_err(|error| error.to_string())?;
                Ok(Value::Bool(true))
            })
            .await
            .map_err(|error| error.to_string())?
        }
        _ => crate::host::file_resource::facts(core.data_directory(), params),
    }
}

pub async fn thumbnail(core: Arc<ContentServer>, mut params: Value) -> Result<Value, String> {
    let token = params
        .get("outputToken")
        .and_then(Value::as_str)
        .ok_or("Thumbnail output token required")?
        .to_owned();
    params
        .as_object_mut()
        .ok_or("Thumbnail request object required")?
        .remove("outputToken");
    let request: crate::services::images::thumbnails::Request =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let meta = tokio::fs::metadata(&request.path)
        .await
        .map_err(|error| error.to_string())?;
    let modified = meta
        .modified()
        .map_err(|error| error.to_string())?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let spec = plain_media::previews::images::ThumbSpec::sanitize(
        &request.path,
        i32::try_from(request.width).map_err(|error| error.to_string())?,
        i32::try_from(request.height).map_err(|error| error.to_string())?,
        85,
        modified,
        meta.len() as i64,
    );
    let outcome = plain_media::previews::images::get_thumbnail(spec.clone())
        .await
        .map_err(|error| error.to_string())?;
    let bytes = match outcome {
        plain_media::previews::images::ThumbOutcome::Generated(bytes) => bytes,
        plain_media::previews::images::ThumbOutcome::Original { data, .. } => data,
    };
    let bytes = if request.center_crop {
        tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
            let image = image::load_from_memory(&bytes)
                .map_err(|error| error.to_string())?
                .resize_to_fill(spec.w, spec.h, image::imageops::FilterType::Lanczos3);
            let mut bytes = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 85)
                .encode_image(&image)
                .map_err(|error| error.to_string())?;
            Ok(bytes)
        })
        .await
        .map_err(|error| error.to_string())??
    } else {
        (*bytes).clone()
    };
    core.headless_state().thumbnails.submit(&token, bytes)?;
    Ok(Value::Bool(true))
}

pub fn media_info(path: &str, kind: plain_rs::enums::DataType) -> Value {
    if kind == plain_rs::enums::DataType::Image {
        let Ok(bytes) = std::fs::read(path) else {
            return Value::Null;
        };
        let Some((width, height)) = plain_rs::utils::image_dimensions::dimensions(&bytes) else {
            return Value::Null;
        };
        let location = plain_rs::utils::image_dimensions::exif_gps(&bytes).map(
            |(latitude, longitude)| serde_json::json!({"latitude":latitude,"longitude":longitude}),
        );
        return serde_json::json!({"width":width,"height":height,"location":location});
    }
    match plain_media::metadata::file_facts::read_file_metadata(Path::new(path), kind) {
        Ok(facts) => {
            serde_json::json!({"width":facts.width,"height":facts.height,"durationMs":facts.duration_ms})
        }
        Err(_) => Value::Null,
    }
}
