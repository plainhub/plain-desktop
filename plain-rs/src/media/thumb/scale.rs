//! Pixel plumbing for the thumbnail engine: the in-memory [`Bitmap`] type,
//! integer box pre-downscaling, EXIF rotation, alpha flattening and the
//! SIMD final resample.

use super::exif::Orientation;

/// A tightly-packed 8-bit bitmap in either RGB or RGBA row-major layout.
#[derive(Debug, Clone)]
pub enum Bitmap {
    Rgb8 { data: Vec<u8>, w: u32, h: u32 },
    Rgba8 { data: Vec<u8>, w: u32, h: u32 },
}

macro_rules! bitmap_accessors {
    ($var:ident) => {
        pub fn $var(&self) -> (&[u8], u32, u32) {
            match self {
                Bitmap::Rgb8 { data, w, h } | Bitmap::Rgba8 { data, w, h } => (data, *w, *h),
            }
        }
    };
}

impl Bitmap {
    bitmap_accessors!(raw);

    pub fn width(&self) -> u32 {
        match self {
            Bitmap::Rgb8 { w, .. } | Bitmap::Rgba8 { w, .. } => *w,
        }
    }

    pub fn height(&self) -> u32 {
        match self {
            Bitmap::Rgb8 { h, .. } | Bitmap::Rgba8 { h, .. } => *h,
        }
    }

    pub fn channels(&self) -> usize {
        match self {
            Bitmap::Rgb8 { .. } => 3,
            Bitmap::Rgba8 { .. } => 4,
        }
    }

    pub fn new_rgb(data: Vec<u8>, w: u32, h: u32) -> Self {
        assert_eq!(data.len(), w as usize * h as usize * 3);
        Bitmap::Rgb8 { data, w, h }
    }

    pub fn new_rgba(data: Vec<u8>, w: u32, h: u32) -> Self {
        assert_eq!(data.len(), w as usize * h as usize * 4);
        Bitmap::Rgba8 { data, w, h }
    }

    /// Convert a decoded `image`-crate image into a Bitmap without extra
    /// copies for the common RGB/RGBA layouts.
    pub fn from_dynamic(img: image::DynamicImage) -> Self {
        use image::DynamicImage;
        match img {
            DynamicImage::ImageRgb8(b) => {
                let (w, h) = (b.width(), b.height());
                Bitmap::new_rgb(b.into_raw(), w, h)
            }
            DynamicImage::ImageRgba8(b) => {
                let (w, h) = (b.width(), b.height());
                Bitmap::new_rgba(b.into_raw(), w, h)
            }
            other => {
                if other.color().has_alpha() {
                    let b = other.to_rgba8();
                    let (w, h) = (b.width(), b.height());
                    Bitmap::new_rgba(b.into_raw(), w, h)
                } else {
                    let b = other.to_rgb8();
                    let (w, h) = (b.width(), b.height());
                    Bitmap::new_rgb(b.into_raw(), w, h)
                }
            }
        }
    }

    /// EXIF orientation applied on the (small, post-resize) thumbnail.
    /// Cheap by construction: it only ever touches thumbnail-sized buffers.
    pub fn apply_orientation(self, o: Orientation) -> Self {
        match o.0 {
            1 | 0 => self,
            2 => self.flip(true),
            3 => self.rotate180(),
            4 => self.flip(false),
            5 => self.transpose(),
            6 => self.rotate90(),
            7 => self.transverse(),
            8 => self.rotate270(),
            _ => self,
        }
    }

    fn rotate90(self) -> Self {
        // 90° CW: dst(x, y) = src(y, H-1-x) — axes swap.
        self.transform_swapping(|x, y, w, h| (y, h - 1 - x, h, w))
    }

    fn rotate270(self) -> Self {
        // 90° CCW: dst(x, y) = src(W-1-y, x) — axes swap.
        self.transform_swapping(|x, y, w, h| (w - 1 - y, x, h, w))
    }

    fn rotate180(self) -> Self {
        self.transform(|x, y, w, h| (w - 1 - x, h - 1 - y, w, h))
    }

    /// Horizontal mirror when `horiz`, else vertical.
    fn flip(self, horiz: bool) -> Self {
        self.transform(|x, y, w, h| {
            if horiz {
                (w - 1 - x, y, w, h)
            } else {
                (x, h - 1 - y, w, h)
            }
        })
    }

    /// Top-left↔bottom-right transpose (EXIF 5).
    fn transpose(self) -> Self {
        self.transform_swapping(|x, y, w, h| (y, x, h, w))
    }

    /// Anti-transpose (EXIF 7): transpose + rotate180.
    fn transverse(self) -> Self {
        self.transpose().rotate180()
    }

    /// Copy pixels through `map(dst_x, dst_y, src_w, src_h) -> (src_x, src_y,
    /// out_w, out_h)` keeping the source dimensions (no axis swap).
    fn transform(self, map: impl Fn(u32, u32, u32, u32) -> (u32, u32, u32, u32)) -> Self {
        let ch = self.channels();
        let (src, w, h) = self.raw_owned();
        let mut dst = vec![0u8; src.len()];
        for y in 0..h {
            for x in 0..w {
                let (sx, sy, ow, oh) = map(x, y, w, h);
                debug_assert_eq!((ow, oh), (w, h));
                let d = px_index(x, y, w, ch);
                let s = px_index(sx, sy, w, ch);
                copy_px(&mut dst[d..], &src[s..], ch);
            }
        }
        rebuild_kind(ch, dst, w, h)
    }

    /// Same as [`Bitmap::transform`] but for maps that swap the axes.
    fn transform_swapping(self, map: impl Fn(u32, u32, u32, u32) -> (u32, u32, u32, u32)) -> Self {
        let ch = self.channels();
        let (src, w, h) = self.raw_owned();
        let (nw, nh) = (h, w);
        let mut dst = vec![0u8; src.len()];
        for y in 0..nh {
            for x in 0..nw {
                let (sx, sy, ow, oh) = map(x, y, w, h);
                debug_assert_eq!((ow, oh), (nw, nh));
                let d = px_index(x, y, nw, ch);
                let s = px_index(sx, sy, w, ch);
                copy_px(&mut dst[d..], &src[s..], ch);
            }
        }
        rebuild_kind(ch, dst, nw, nh)
    }

    fn raw_owned(self) -> (Vec<u8>, u32, u32) {
        let (w, h) = (self.width(), self.height());
        let data = match self {
            Bitmap::Rgb8 { data, .. } | Bitmap::Rgba8 { data, .. } => data,
        };
        (data, w, h)
    }

    /// Flatten alpha onto a white background, yielding RGB.
    pub fn flatten_to_rgb(self) -> Bitmap {
        match self {
            Bitmap::Rgb8 { .. } => self,
            Bitmap::Rgba8 { data, w, h } => {
                let mut out = Vec::with_capacity(data.len() / 4 * 3);
                for px in data.chunks_exact(4) {
                    let a = px[3] as u32;
                    out.push(((px[0] as u32 * a + (255 - a) * 255) / 255) as u8);
                    out.push(((px[1] as u32 * a + (255 - a) * 255) / 255) as u8);
                    out.push(((px[2] as u32 * a + (255 - a) * 255) / 255) as u8);
                }
                Bitmap::new_rgb(out, w, h)
            }
        }
    }
}

fn rebuild_kind(ch: usize, data: Vec<u8>, w: u32, h: u32) -> Bitmap {
    if ch == 4 {
        Bitmap::new_rgba(data, w, h)
    } else {
        Bitmap::new_rgb(data, w, h)
    }
}

/// Copy one pixel between flat buffers whose indices the caller computed.
#[inline]
fn copy_px(dst: &mut [u8], src: &[u8], ch: usize) {
    dst[..ch].copy_from_slice(&src[..ch]);
}

/// Flat-buffer pixel index helper for transform maps.
#[inline]
fn px_index(x: u32, y: u32, stride: u32, ch: usize) -> usize {
    (y as usize * stride as usize + x as usize) * ch
}

/// Two-pass integer box downscale by `f` (≥ 2). Edge boxes average over the
/// pixels they actually cover, so the result is exactly a box filter.
pub fn box_downscale(bm: &Bitmap, f: u32) -> Bitmap {
    let (src, w, h) = bm.raw();
    let ch = bm.channels();
    debug_assert!(f >= 2);
    let (ow, oh) = (w.div_ceil(f), h.div_ceil(f));

    // Pass 1 (vertical): accumulate `f` source rows per output row.
    let vw = w as usize;
    let mut vert = vec![0u32; vw * oh as usize * ch];
    for oy in 0..oh as usize {
        let row = &mut vert[oy * vw * ch..(oy + 1) * vw * ch];
        let y0 = oy * f as usize;
        let y1 = ((oy + 1) * f as usize).min(h as usize);
        for y in y0..y1 {
            let srow = &src[y * vw * ch..(y + 1) * vw * ch];
            for (acc, v) in row.iter_mut().zip(srow) {
                *acc += *v as u32;
            }
        }
        let n = (y1 - y0) as u32;
        for acc in row.iter_mut() {
            // Scale to sum-per-single-row so pass 2 can divide once at the end.
            *acc = (*acc + n / 2) / n;
        }
    }

    // Pass 2 (horizontal): accumulate `f` columns per output column.
    let mut dst = vec![0u8; ow as usize * oh as usize * ch];
    for oy in 0..oh as usize {
        let vrow = &vert[oy * vw * ch..(oy + 1) * vw * ch];
        let orow = &mut dst[oy * ow as usize * ch..(oy + 1) * ow as usize * ch];
        let mut acc = vec![0u32; ch];
        for ox in 0..ow as usize {
            acc.iter_mut().for_each(|a| *a = 0);
            let x0 = ox * f as usize;
            let x1 = ((ox + 1) * f as usize).min(w as usize);
            let n = (x1 - x0) as u32;
            for x in x0..x1 {
                for c in 0..ch {
                    acc[c] += vrow[x * ch + c];
                }
            }
            for c in 0..ch {
                orow[ox * ch + c] = ((acc[c] + n / 2) / n).min(255) as u8;
            }
        }
    }

    match bm {
        Bitmap::Rgb8 { .. } => Bitmap::new_rgb(dst, ow, oh),
        Bitmap::Rgba8 { .. } => Bitmap::new_rgba(dst, ow, oh),
    }
}

/// Full resize strategy: optional single box pre-pass when the downscale
/// ratio is large, then the SIMD convolution resample to the exact target.
pub fn resize_to(bm: Bitmap, tw: u32, th: u32) -> Bitmap {
    let (w, h) = (bm.width(), bm.height());
    if tw == 0 || th == 0 || (tw == w && th == h) {
        return bm;
    }

    // Choose the largest integer factor that keeps the intermediate at or
    // above the target in both axes (so the final pass still downsamples),
    // capped so one box pass is enough.
    let f = w
        .checked_div(tw)
        .unwrap_or(0)
        .min(h.checked_div(th).unwrap_or(0))
        .min(64);
    let mid = if f >= 2 && (w / f >= tw && h / f >= th) {
        box_downscale(&bm, f)
    } else {
        bm
    };

    fir_resize(mid, tw, th)
}

/// Final resample via `fast_image_resize` (pure Rust, SIMD convolution).
/// Bilinear for small thumbnails, Lanczos3 when quality matters more.
fn fir_resize(bm: Bitmap, tw: u32, th: u32) -> Bitmap {
    use fast_image_resize::{PixelType, ResizeAlg, ResizeOptions, Resizer};

    let (w, h) = (bm.width(), bm.height());
    if tw == w && th == h {
        return bm;
    }
    let ch = bm.channels();
    let (data, _, _) = bm.raw_owned();
    let pixel_type = if ch == 4 {
        PixelType::U8x4
    } else {
        PixelType::U8x3
    };
    let src = fast_image_resize::images::Image::from_vec_u8(w, h, data, pixel_type)
        .expect("bitmap buffer matches dimensions");
    let mut dst = fast_image_resize::images::Image::new(tw, th, pixel_type);
    let mut resizer = Resizer::new();
    let filter = if tw.max(th) < 200 {
        fast_image_resize::FilterType::Bilinear
    } else {
        fast_image_resize::FilterType::Lanczos3
    };
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(filter));
    resizer
        .resize(&src, &mut dst, &options)
        .expect("fast_image_resize never fails on valid buffers");
    let out = dst.into_vec();
    if ch == 4 {
        Bitmap::new_rgba(out, tw, th)
    } else {
        Bitmap::new_rgb(out, tw, th)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/media/thumb/scale.rs"]
mod tests;
