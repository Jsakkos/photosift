/// Normalized 0-1 point.
#[derive(Debug, Clone, Copy)]
pub struct NormPoint {
    pub x: f64,
    pub y: f64,
}

/// Normalized 0-1 bbox.
#[derive(Debug, Clone, Copy)]
pub struct NormBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Pixel crop rect, always in-bounds for the given image dimensions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelCrop {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Crop a square region around an eye landmark sized at 15% of face bbox
/// width, clamped to image bounds. All inputs are normalized 0-1; output is
/// pixel coordinates on an `img_w × img_h` image.
pub fn eye_crop_pixels(
    eye: &NormPoint,
    face: &NormBox,
    img_w: u32,
    img_h: u32,
) -> PixelCrop {
    let face_w_px = face.w * img_w as f64;
    let side_px = (face_w_px * 0.15).max(8.0) as i32;
    let half = side_px / 2;

    let cx = (eye.x * img_w as f64) as i32;
    let cy = (eye.y * img_h as f64) as i32;

    let mut x = cx - half;
    let mut y = cy - half;
    let mut w = side_px;
    let mut h = side_px;

    if x < 0 {
        w += x;
        x = 0;
    }
    if y < 0 {
        h += y;
        y = 0;
    }
    if x + w > img_w as i32 {
        w = img_w as i32 - x;
    }
    if y + h > img_h as i32 {
        h = img_h as i32 - y;
    }
    let side = w.min(h).max(1);

    PixelCrop {
        x: x as u32,
        y: y as u32,
        w: side as u32,
        h: side as u32,
    }
}

use anyhow::Result;
use image::GrayImage;

pub trait EyeStateProvider: Send + Sync {
    /// Returns 0 (closed) or 1 (open).
    fn classify(&self, crop: &GrayImage) -> Result<i32>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crop_is_square_and_in_bounds() {
        let eye = NormPoint { x: 0.3, y: 0.4 };
        let face = NormBox { x: 0.2, y: 0.2, w: 0.3, h: 0.4 };
        let crop = eye_crop_pixels(&eye, &face, 1000, 800);
        assert_eq!(crop.w, crop.h, "crop must be square");
        assert!(crop.x + crop.w <= 1000);
        assert!(crop.y + crop.h <= 800);
    }

    #[test]
    fn test_crop_size_is_15_percent_of_face_width() {
        let eye = NormPoint { x: 0.5, y: 0.5 };
        let face = NormBox { x: 0.3, y: 0.3, w: 0.4, h: 0.5 };
        let crop = eye_crop_pixels(&eye, &face, 1000, 1000);
        // 40% of 1000 = 400 px face width, 15% of 400 = 60.
        assert!((crop.w as i32 - 60).abs() <= 1, "expected ~60, got {}", crop.w);
    }

    #[test]
    fn test_crop_clamps_at_image_edges() {
        let eye = NormPoint { x: 0.01, y: 0.01 };
        let face = NormBox { x: 0.0, y: 0.0, w: 0.2, h: 0.2 };
        let crop = eye_crop_pixels(&eye, &face, 1000, 1000);
        assert_eq!(crop.x, 0);
        assert_eq!(crop.y, 0);
    }
}
