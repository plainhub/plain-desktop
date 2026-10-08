use super::manifest::{Interpolation, Layout, Preprocess, Resize};
use image::{ImageDecoder, ImageReader, imageops::FilterType};
use std::path::Path;

pub fn tensor(path: &Path, config: &Preprocess) -> Result<Option<Vec<f32>>, String> {
    let mut reader = ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(128 * 1024 * 1024);
    limits.max_image_width = Some(32768);
    limits.max_image_height = Some(32768);
    reader.limits(limits);
    let mut decoder = match reader.into_decoder() {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let mut image = match image::DynamicImage::from_decoder(decoder) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    if image.width() < 64 || image.height() < 64 {
        return Ok(None);
    }
    image.apply_orientation(orientation);
    let size = config.size;
    let filter = match config.interpolation {
        Interpolation::Bilinear => FilterType::Triangle,
        Interpolation::Bicubic => FilterType::CatmullRom,
    };
    let image = match config.resize {
        Resize::Stretch => image.resize_exact(size, size, filter),
        Resize::CenterCrop => {
            let scale = size as f64 / image.width().min(image.height()) as f64;
            let width = (image.width() as f64 * scale).floor() as u32;
            let height = (image.height() as f64 * scale).floor() as u32;
            if width as u64 * height as u64 > 32 * 1024 * 1024 {
                return Err("Image resize exceeds memory limit".into());
            }
            let resized = image.resize_exact(width.max(size), height.max(size), filter);
            resized.crop_imm(
                (resized.width() - size) / 2,
                (resized.height() - size) / 2,
                size,
                size,
            )
        }
    }
    .to_rgb8();
    let area = (size * size) as usize;
    let mut tensor = vec![0.0; area * 3];
    for (i, pixel) in image.pixels().enumerate() {
        for c in 0..3 {
            let index = match config.layout {
                Layout::Nchw => c * area + i,
                Layout::Nhwc => i * 3 + c,
            };
            tensor[index] = (pixel[c] as f32 / 255.0 - config.mean[c]) / config.std[c];
        }
    }
    Ok(Some(tensor))
}
