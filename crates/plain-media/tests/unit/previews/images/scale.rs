//! Unit tests for `src/media/thumb_engine/scale.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

fn rgb(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 3]) -> Bitmap {
    let mut data = Vec::with_capacity(w as usize * h as usize * 3);
    for y in 0..h {
        for x in 0..w {
            data.extend_from_slice(&f(x, y));
        }
    }
    Bitmap::new_rgb(data, w, h)
}

#[test]
fn box_downscale_exact_multiple() {
    // 4x4 → 2x2 with factor 2; each output = mean of a 2x2 block.
    let bm = rgb(4, 4, |x, y| [(x * 10) as u8, (y * 10) as u8, 7]);
    let out = box_downscale(&bm, 2);
    assert_eq!((out.width(), out.height()), (2, 2));
    let (d, _, _) = out.raw();
    // Block (0,0): pixels (0,0),(1,0),(0,1),(1,1) → x-mean 5, y-mean 5.
    assert_eq!(&d[..3], &[5, 5, 7]);
    // Block (1,1): x in {2,3} → mean 25, y in {2,3} → mean 25.
    assert_eq!(&d[3 * 3..3 * 4], &[25, 25, 7]);
}

#[test]
fn box_downscale_non_multiple_edges() {
    // 5x3 → 2x1 with factor 3: output averages 3 and 2 columns, all rows.
    let bm = rgb(5, 3, |x, _| [x as u8, x as u8, x as u8]);
    let out = box_downscale(&bm, 3);
    assert_eq!((out.width(), out.height()), (2, 1));
    let (d, _, _) = out.raw();
    // Cols 0..3 mean = (0+1+2)/3 = 1; cols 3..5 mean = (3+4)/2 = 3.5 → 4.
    assert_eq!(&d[..3], &[1, 1, 1]);
    assert_eq!(&d[3..6], &[4, 4, 4]);
}

#[test]
fn box_downscale_rgba_channels() {
    let mut data = Vec::new();
    for i in 0..4u8 {
        data.extend_from_slice(&[i * 10, i * 10 + 1, i * 10 + 2, 255]);
    }
    let bm = Bitmap::new_rgba(data, 2, 2);
    let out = box_downscale(&bm, 2);
    let (d, _, _) = out.raw();
    assert_eq!(d, &[15, 16, 17, 255]);
}

#[test]
fn box_downscale_constant_image_is_constant() {
    let bm = rgb(64, 64, |_, _| [200, 100, 50]);
    let out = box_downscale(&bm, 8);
    assert!(out.raw().0.chunks_exact(3).all(|px| px == [200, 100, 50]));
}

#[test]
fn rotate_all_orientations_roundtrip() {
    let bm = rgb(3, 2, |x, y| [x as u8, y as u8, (x + y) as u8]);
    // Rotating four times by 90° returns the original.
    let r = bm
        .clone()
        .apply_orientation(Orientation(6))
        .apply_orientation(Orientation(6))
        .apply_orientation(Orientation(6))
        .apply_orientation(Orientation(6));
    assert_eq!(r.raw().0, bm.raw().0);
    assert_eq!((r.width(), r.height()), (3, 2));

    // Orientation 6 swaps axes once.
    let r6 = bm.clone().apply_orientation(Orientation(6));
    assert_eq!((r6.width(), r6.height()), (2, 3));
    // Known value: dst(0,0) = src(y=H-1-0=1? no: src(sx=y=0, sy=H-1-x=1)) = (0,1,1).
    assert_eq!(&r6.raw().0[..3], &[0, 1, 1]);

    // Orientation 2 (h-flip): first pixel of each row = last source pixel.
    let r2 = bm.clone().apply_orientation(Orientation(2));
    assert_eq!(&r2.raw().0[..3], &[2, 0, 2]);
    // Orientation 3 = 180°: first pixel = last source pixel mirrored.
    let r3 = bm.clone().apply_orientation(Orientation(3));
    assert_eq!(&r3.raw().0[..3], &[2, 1, 3]);
}

#[test]
fn transpose_swaps_axes() {
    let bm = rgb(3, 2, |x, y| [x as u8, y as u8, 0]);
    let t = bm.clone().apply_orientation(Orientation(5));
    assert_eq!((t.width(), t.height()), (2, 3));
    // Transpose: dst(x,y) = src(y,x) → dst(0,0)=src(0,0), dst(1,0)=src(0,1).
    assert_eq!(&t.raw().0[..3], &[0, 0, 0]);
    assert_eq!(&t.raw().0[3..6], &[0, 1, 0]);
}

#[test]
fn flatten_alpha_composites_white() {
    let data = vec![255, 0, 0, 0, 10, 20, 30, 128];
    let bm = Bitmap::new_rgba(data, 2, 1);
    let out = bm.flatten_to_rgb();
    let (d, _, _) = out.raw();
    assert_eq!(
        d,
        &[
            255,
            255,
            255,
            ((10 * 128 + 127 * 255) / 255) as u8,
            ((20 * 128 + 127 * 255) / 255) as u8,
            ((30 * 128 + 127 * 255) / 255) as u8
        ]
    );
}

#[test]
fn resize_to_large_ratio_uses_box_then_fir() {
    // 512x512 → 16x16 exercises the box pre-pass + final bilinear.
    let bm = rgb(512, 512, |x, y| [x as u8, y as u8, 128]);
    let out = resize_to(bm.clone(), 16, 16);
    assert_eq!((out.width(), out.height()), (16, 16));
    // Top-left output pixel averages source x=0..31 whose gradient
    // (x as u8 over 512 px wraps) has mean 15.5 → ~15/16.
    let (d, _, _) = out.raw();
    assert!(
        (d[0] as i32 - 15).abs() <= 2,
        "x mean approx 15, got {}",
        d[0]
    );
}

#[test]
fn resize_to_upscale_identity_edges() {
    let bm = rgb(10, 10, |_, _| [5, 5, 5]);
    let same = resize_to(bm.clone(), 10, 10);
    assert_eq!(same.raw().0, bm.raw().0);
    // Upscale request (target bigger than source) still resamples.
    let up = resize_to(bm.clone(), 20, 20);
    assert_eq!((up.width(), up.height()), (20, 20));
    assert!(up.raw().0.iter().all(|v| (5..=6).contains(v)));
    // Extreme aspect source.
    let wide = rgb(1000, 2, |x, _| [(x % 256) as u8, 0, 0]);
    let out = resize_to(wide.clone(), 50, 50);
    assert_eq!((out.width(), out.height()), (50, 50));
}
