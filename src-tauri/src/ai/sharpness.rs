use image::GrayImage;

/// Discrete Laplacian kernel: [[0,1,0],[1,-4,1],[0,1,0]]
/// Variance of the filtered image is the "sharpness" proxy. Higher = sharper.
pub fn laplacian_variance(img: &GrayImage) -> f64 {
    let (w, h) = img.dimensions();
    if w < 3 || h < 3 {
        return 0.0;
    }

    let get = |x: u32, y: u32| img.get_pixel(x, y).0[0] as f64;
    let n = ((w - 2) * (h - 2)) as f64;

    let mut sum = 0.0_f64;
    let mut sum_sq = 0.0_f64;
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let lap = get(x, y - 1) + get(x - 1, y) + get(x + 1, y) + get(x, y + 1)
                - 4.0 * get(x, y);
            sum += lap;
            sum_sq += lap * lap;
        }
    }
    let mean = sum / n;
    (sum_sq / n) - (mean * mean)
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

/// Empirical constant. Measured on a real D750 shoot on 2026-04-18:
/// full-resolution embedded-JPEG previews top out at raw Laplacian
/// variance ~75 for tack-sharp portraits (rare spikes to 300+ on
/// high-detail scenes like foliage), moderate 15-30, soft/OOF 2-8.
/// Mapping variance=50 to sharpness=100 gives a useful culling spread:
/// tack-sharp photos saturate near 100, sharp 60-80, moderate 30-50,
/// soft 0-15. Users can then set hideSoftThreshold around 20-30 and
/// meaningfully filter.
pub const CALIBRATION_FULL_SCALE: f64 = 50.0;

pub fn normalize_sharpness(variance: f64) -> f64 {
    if variance <= 0.0 {
        return 0.0;
    }
    let scaled = variance / CALIBRATION_FULL_SCALE * 100.0;
    if scaled > 100.0 {
        100.0
    } else {
        scaled
    }
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
    fn test_normalize_score_clamps_to_0_100() {
        assert_eq!(normalize_sharpness(0.0), 0.0);
        assert_eq!(normalize_sharpness(-5.0), 0.0);
        assert!((normalize_sharpness(CALIBRATION_FULL_SCALE) - 100.0).abs() < 1.0);
        assert_eq!(normalize_sharpness(CALIBRATION_FULL_SCALE * 10.0), 100.0);
    }
}
