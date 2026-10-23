//! Sharpness (Laplacian variance) and exposure statistics from grayscale pixels.

use super::faces::Face;
use crate::media::render::RgbImage;

/// 0-255 values at or above this count as blown highlights.
const CLIP_HI_ABOVE: f32 = 250.0;
/// 0-255 values below this count as crushed shadows.
const CLIP_LO_BELOW: f32 = 5.0;
/// A face box smaller than this (px per side) is too small to judge focus on.
const MIN_FACE_PX: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quality {
    /// Laplacian variance of the whole frame; only comparable between photos
    /// measured at the same resolution.
    pub sharpness: f64,
    /// Max Laplacian variance over the supplied face boxes.
    pub face_sharpness: Option<f64>,
    /// Mean luma, 0-255.
    pub mean_luma: f64,
    /// Fraction of pixels that are blown out.
    pub clip_hi: f64,
    /// Fraction of pixels that are crushed to black.
    pub clip_lo: f64,
}

/// Rec. 601 luma, 0-255.
pub fn gray_of(img: &RgbImage) -> Vec<f32> {
    img.px
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| 0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2]))
        .collect()
}

/// Variance of the 4-neighbour Laplacian over the interior of a `w`x`h` buffer.
pub fn laplacian_variance(g: &[f32], w: usize, h: usize) -> f64 {
    if w < 3 || h < 3 || g.len() < w * h {
        return 0.0;
    }
    let (mut sum, mut sq, mut n) = (0f64, 0f64, 0f64);
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = y * w + x;
            let v = f64::from(g[i - 1] + g[i + 1] + g[i - w] + g[i + w] - 4.0 * g[i]);
            sum += v;
            sq += v * v;
            n += 1.0;
        }
    }
    sq / n - (sum / n).powi(2)
}

/// Pixel rectangle `(x0, y0, x1, y1)` (x1/y1 exclusive) of a normalized face box.
fn face_rect(face: &Face, w: usize, h: usize) -> Option<(usize, usize, usize, usize)> {
    let (x, y, bw, bh) = face.bbox;
    let px = |v: f32, dim: usize| (v.clamp(0.0, 1.0) * dim as f32).round() as usize;
    let (x0, y0) = (px(x, w), px(y, h));
    let (x1, y1) = (px(x + bw, w).min(w), px(y + bh, h).min(h));
    (x1 > x0 + MIN_FACE_PX && y1 > y0 + MIN_FACE_PX).then_some((x0, y0, x1, y1))
}

fn crop(g: &[f32], w: usize, (x0, y0, x1, y1): (usize, usize, usize, usize)) -> Vec<f32> {
    let mut out = Vec::with_capacity((x1 - x0) * (y1 - y0));
    for y in y0..y1 {
        out.extend_from_slice(&g[y * w + x0..y * w + x1]);
    }
    out
}

/// Laplacian variance of one face's box in a grayscale buffer.
pub fn face_sharpness_in(gray: &[f32], w: usize, h: usize, face: &Face) -> Option<f64> {
    let rect = face_rect(face, w, h)?;
    let (cw, ch) = (rect.2 - rect.0, rect.3 - rect.1);
    Some(laplacian_variance(&crop(gray, w, rect), cw, ch))
}

/// Same, straight from RGB pixels without converting the whole frame.
pub fn face_sharpness(img: &RgbImage, face: &Face) -> Option<f64> {
    let (w, h) = (img.w as usize, img.h as usize);
    let (x0, y0, x1, y1) = face_rect(face, w, h)?;
    let mut g = Vec::with_capacity((x1 - x0) * (y1 - y0));
    for y in y0..y1 {
        for p in img.px[(y * w + x0) * 3..(y * w + x1) * 3]
            .as_chunks::<3>()
            .0
        {
            g.push(0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2]));
        }
    }
    Some(laplacian_variance(&g, x1 - x0, y1 - y0))
}

/// Global sharpness and exposure of a grayscale frame. `face_sharpness` is
/// measured on this same buffer; the enrichment job passes no faces here and
/// measures them on the full-size preview instead (a face is small at 1024 px).
pub fn measure(gray: &[f32], w: usize, h: usize, faces: &[Face]) -> Quality {
    let n = (w * h).max(1) as f64;
    let mean = gray.iter().map(|&v| f64::from(v)).sum::<f64>() / n;
    let hi = gray.iter().filter(|&&v| v > CLIP_HI_ABOVE).count() as f64 / n;
    let lo = gray.iter().filter(|&&v| v < CLIP_LO_BELOW).count() as f64 / n;
    let face_sharpness = faces
        .iter()
        .filter_map(|f| face_sharpness_in(gray, w, h, f))
        .reduce(f64::max);
    Quality {
        sharpness: laplacian_variance(gray, w, h),
        face_sharpness,
        mean_luma: mean,
        clip_hi: hi,
        clip_lo: lo,
    }
}
