use image::GrayImage;
use rayon::prelude::*;

/// Discrete Laplacian kernel: [[0,1,0],[1,-4,1],[0,1,0]]
/// Variance of the filtered image is the "sharpness" proxy. Higher = sharper.
///
/// Hot-path implementation: reads the raw `Vec<u8>` buffer directly and
/// parallelises the outer row loop via rayon. On a 24 MP (6016×4016)
/// preview this drops from ~600 ms single-threaded `get_pixel` to ~10–20 ms
/// on 8 cores, which matters because the AI worker calls it per photo.
/// For tiny crops (eye patches) the parallel overhead is negligible —
/// rayon's fork work-steals opportunistically.
pub fn laplacian_variance(img: &GrayImage) -> f64 {
    let (w, h) = img.dimensions();
    if w < 3 || h < 3 {
        return 0.0;
    }
    let stride = w as usize;
    let data = img.as_raw();
    let n = ((w - 2) * (h - 2)) as f64;

    let (sum, sum_sq): (f64, f64) = (1..h as usize - 1)
        .into_par_iter()
        .map(|y| {
            let row_up = (y - 1) * stride;
            let row_mid = y * stride;
            let row_dn = (y + 1) * stride;
            let mut row_sum = 0.0_f64;
            let mut row_sum_sq = 0.0_f64;
            for x in 1..stride - 1 {
                let center = data[row_mid + x] as f64;
                let lap = data[row_up + x] as f64
                    + data[row_mid + x - 1] as f64
                    + data[row_mid + x + 1] as f64
                    + data[row_dn + x] as f64
                    - 4.0 * center;
                row_sum += lap;
                row_sum_sq += lap * lap;
            }
            (row_sum, row_sum_sq)
        })
        .reduce(|| (0.0, 0.0), |a, b| (a.0 + b.0, a.1 + b.1));

    let mean = sum / n;
    (sum_sq / n) - (mean * mean)
}

/// Tenengrad: mean of squared Sobel gradient magnitude over the image,
/// skipping the 1px border to avoid kernel overhang. Responds to edge
/// *strength* regardless of local-window variance, so partially-focused
/// low-variance textures (tree foliage, feathers, distant foliage) score
/// meaningfully where `laplacian_variance` would collapse them into the
/// noise floor.
pub fn tenengrad(img: &GrayImage) -> f64 {
    let (w, h) = img.dimensions();
    if w < 3 || h < 3 {
        return 0.0;
    }
    let get = |x: u32, y: u32| img.get_pixel(x, y).0[0] as f64;
    let n = ((w - 2) * (h - 2)) as f64;
    let mut sum = 0.0_f64;
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            // Sobel Gx = [[-1,0,1],[-2,0,2],[-1,0,1]]
            let gx = -get(x - 1, y - 1) + get(x + 1, y - 1)
                + -2.0 * get(x - 1, y) + 2.0 * get(x + 1, y)
                + -get(x - 1, y + 1) + get(x + 1, y + 1);
            // Sobel Gy = [[-1,-2,-1],[0,0,0],[1,2,1]]
            let gy = -get(x - 1, y - 1) - 2.0 * get(x, y - 1) - get(x + 1, y - 1)
                + get(x - 1, y + 1) + 2.0 * get(x, y + 1) + get(x + 1, y + 1);
            sum += gx * gx + gy * gy;
        }
    }
    sum / n
}

/// Per-tile Tenengrad over a `cols × rows` grid. Mirrors
/// `tiled_laplacian`'s row-major layout so the heatmap consumer is
/// interchangeable.
pub fn tiled_tenengrad(img: &GrayImage, cols: u32, rows: u32) -> Vec<f64> {
    let (w, h) = img.dimensions();
    let tw = w / cols;
    let th = h / rows;
    let mut out = Vec::with_capacity((cols * rows) as usize);
    for ry in 0..rows {
        for cx in 0..cols {
            let x0 = cx * tw;
            let y0 = ry * th;
            let sub = image::imageops::crop_imm(img, x0, y0, tw, th).to_image();
            out.push(tenengrad(&sub));
        }
    }
    out
}

/// Compute Laplacian variance for each tile in a `cols × rows` grid.
/// Returns row-major flat vec, length `cols * rows`.
pub fn tiled_laplacian(img: &GrayImage, cols: u32, rows: u32) -> Vec<f64> {
    let (w, h) = img.dimensions();
    let tw = w / cols;
    let th = h / rows;
    let mut out = Vec::with_capacity((cols * rows) as usize);
    for ry in 0..rows {
        for cx in 0..cols {
            let x0 = cx * tw;
            let y0 = ry * th;
            let sub = image::imageops::crop_imm(img, x0, y0, tw, th).to_image();
            out.push(laplacian_variance(&sub));
        }
    }
    out
}

/// Half-saturation point of the sharpness curve, in raw Laplacian variance.
///
/// Measured on a D750 shoot on 2026-04-18: embedded-JPEG previews at
/// full resolution give raw Laplacian variance ~75 for tack-sharp
/// portraits (spikes to 300+ on high-detail scenes like foliage),
/// moderate 15-30, soft/OOF 2-8. The old linear mapping (variance=50 → 100)
/// saturated constantly on real data — high-detail scenes all collapsed
/// to 100 and ranking stopped working. The soft curve below is a
/// Michaelis–Menten form that approaches 100 asymptotically, so the
/// most-detailed photo in a shoot always scores higher than the next.
/// Variance of `CALIBRATION_FULL_SCALE` maps to exactly 50 on the
/// normalized axis.
pub const CALIBRATION_FULL_SCALE: f64 = 50.0;

pub fn normalize_sharpness(variance: f64) -> f64 {
    if variance <= 0.0 {
        return 0.0;
    }
    // Soft saturation: never reaches 100 but preserves rank ordering at
    // the high end. variance=K → 50, variance=5K → ~83, variance=10K → ~91.
    100.0 * variance / (variance + CALIBRATION_FULL_SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Luma};

    fn checkerboard(w: u32, h: u32, cell: u32) -> GrayImage {
        ImageBuffer::from_fn(w, h, |x, y| {
            let c = ((x / cell) + (y / cell)) % 2;
            Luma([if c == 0 { 0 } else { 255 }])
        })
    }

    fn flat(w: u32, h: u32, v: u8) -> GrayImage {
        ImageBuffer::from_fn(w, h, |_, _| Luma([v]))
    }

    #[test]
    fn test_flat_image_has_zero_sharpness() {
        let img = flat(64, 64, 128);
        let s = laplacian_variance(&img);
        assert!(s.abs() < 1e-6, "flat image should have ~0 variance, got {}", s);
    }

    #[test]
    fn test_checkerboard_has_high_sharpness() {
        let sharp = checkerboard(64, 64, 2);
        let softer = checkerboard(64, 64, 16);
        let s_sharp = laplacian_variance(&sharp);
        let s_softer = laplacian_variance(&softer);
        assert!(s_sharp > s_softer, "2px checkerboard ({}) should be sharper than 16px ({})", s_sharp, s_softer);
    }

    #[test]
    fn test_tiled_grid_shape() {
        let img = checkerboard(128, 96, 2);
        let grid = tiled_laplacian(&img, 8, 8);
        assert_eq!(grid.len(), 8 * 8);
        for (i, v) in grid.iter().enumerate() {
            assert!(*v > 0.0, "tile {} had variance {}", i, v);
        }
    }

    #[test]
    fn test_normalize_score_soft_saturation() {
        // Zero and negative collapse to 0.
        assert_eq!(normalize_sharpness(0.0), 0.0);
        assert_eq!(normalize_sharpness(-5.0), 0.0);
        // At the half-saturation point, score is 50 (not 100 as with the
        // old linear mapping).
        assert!((normalize_sharpness(CALIBRATION_FULL_SCALE) - 50.0).abs() < 0.01);
        // Very sharp images approach but never reach 100, so rank order
        // at the top of the scale is preserved.
        let huge = normalize_sharpness(CALIBRATION_FULL_SCALE * 100.0);
        assert!(huge > 98.0, "CAL*100 should be >98, got {}", huge);
        assert!(huge < 100.0, "CAL*100 should not saturate, got {}", huge);
        // Monotonic: larger variance → strictly larger score.
        let a = normalize_sharpness(100.0);
        let b = normalize_sharpness(200.0);
        assert!(b > a, "expected {} > {}", b, a);
    }

    fn vertical_step_edge(w: u32, h: u32) -> GrayImage {
        ImageBuffer::from_fn(w, h, |x, _| {
            Luma([if x < w / 2 { 0 } else { 255 }])
        })
    }

    #[test]
    fn test_tenengrad_zero_on_flat_image() {
        let img = flat(64, 64, 128);
        let s = tenengrad(&img);
        assert!(s.abs() < 1e-6, "flat image should have ~0 tenengrad, got {}", s);
    }

    #[test]
    fn test_tenengrad_detects_edges() {
        let flat_img = flat(64, 64, 128);
        let edge_img = vertical_step_edge(64, 64);
        let s_flat = tenengrad(&flat_img);
        let s_edge = tenengrad(&edge_img);
        assert!(
            s_edge > s_flat + 100.0,
            "step-edge tenengrad ({}) should far exceed flat ({})",
            s_edge,
            s_flat
        );
    }

    #[test]
    fn test_tenengrad_higher_for_sharp_than_soft_checkerboard() {
        let sharp = checkerboard(64, 64, 2);
        let softer = checkerboard(64, 64, 16);
        let s_sharp = tenengrad(&sharp);
        let s_softer = tenengrad(&softer);
        assert!(
            s_sharp > s_softer,
            "2px checkerboard tenengrad ({}) should exceed 16px ({})",
            s_sharp,
            s_softer,
        );
    }

    #[test]
    fn test_tiled_tenengrad_dimensions() {
        // 48 cols * 32 rows matches the target grid for the heatmap path.
        let img = checkerboard(192, 128, 2);
        let grid = tiled_tenengrad(&img, 48, 32);
        assert_eq!(grid.len(), 48 * 32);
        for v in &grid {
            assert!(*v >= 0.0, "tenengrad must be non-negative, got {}", v);
        }
    }
}
