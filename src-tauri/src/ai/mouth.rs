use crate::ai::eye::{NormBox, PixelCrop};
use anyhow::Result;
use image::GrayImage;
use std::sync::atomic::{AtomicI32, Ordering};

/// Narrative-Select-style mouth signal. `mouth_open` is binary (0/1),
/// `smile_confidence` is a `[0.0, 1.0]` probability. Providers that
/// only classify open/closed may return a constant smile of 0.5
/// (unknown); the UI should treat smile as "informational" until a
/// smile-trained classifier ships.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MouthState {
    pub mouth_open: i32,
    pub smile_confidence: f64,
}

pub trait MouthStateProvider: Send + Sync {
    fn classify(&self, crop: &GrayImage) -> Result<MouthState>;
}

/// Deterministic alternating mock. Same shape as `MockEyeProvider` so
/// UI / tests can differentiate "classifier ran" from "real signal"
/// by checking `MouthProviderKind == Onnx`.
pub struct MockMouthProvider {
    counter: AtomicI32,
}

impl Default for MockMouthProvider {
    fn default() -> Self {
        Self {
            counter: AtomicI32::new(0),
        }
    }
}

impl MouthStateProvider for MockMouthProvider {
    fn classify(&self, _: &GrayImage) -> Result<MouthState> {
        let n = self.counter.fetch_add(1, Ordering::SeqCst);
        Ok(MouthState {
            mouth_open: n % 2,
            smile_confidence: 0.5,
        })
    }
}

/// Convert a normalized face bbox to a pixel-space crop, clamped to the
/// image bounds. Used for face-holistic mouth/expression classifiers
/// (FER+, 7-class FER) that want the full face as input rather than a
/// mouth-only patch.
///
/// We inflate the bbox by 10 % on each side so the tightly-cropped YuNet
/// output — which can clip eyebrows and chin — grows out to include the
/// cheek/forehead context that FER-family models were trained on.
pub fn face_crop_pixels(face: &NormBox, img_w: u32, img_h: u32) -> PixelCrop {
    let pad = 0.10;
    let face_x = (face.x - face.w * pad).max(0.0);
    let face_y = (face.y - face.h * pad).max(0.0);
    let face_w = face.w * (1.0 + 2.0 * pad);
    let face_h = face.h * (1.0 + 2.0 * pad);

    let mut x = (face_x * img_w as f64) as i32;
    let mut y = (face_y * img_h as f64) as i32;
    let mut w = (face_w * img_w as f64) as i32;
    let mut h = (face_h * img_h as f64) as i32;

    if x < 0 { w += x; x = 0; }
    if y < 0 { h += y; y = 0; }
    if x + w > img_w as i32 { w = img_w as i32 - x; }
    if y + h > img_h as i32 { h = img_h as i32 - y; }
    PixelCrop {
        x: x.max(0) as u32,
        y: y.max(0) as u32,
        w: w.max(1) as u32,
        h: h.max(1) as u32,
    }
}

/// Geometric mouth crop derived from a face bounding box. YuNet only
/// surfaces eye landmarks out of its five 5-point keypoint set; the
/// mouth corners are there in the network output but our `DetectedFace`
/// drops them to keep the struct lean. Until we extend the detector
/// to preserve mouth landmarks, we approximate with the face bbox:
/// the mouth sits roughly in the lower third, horizontally centered,
/// with ~60% of the face width and ~20% of the height.
pub fn mouth_crop_pixels(face: &NormBox, img_w: u32, img_h: u32) -> PixelCrop {
    let face_w_px = (face.w * img_w as f64).max(1.0);
    let face_h_px = (face.h * img_h as f64).max(1.0);

    let crop_w = (face_w_px * 0.60) as i32;
    let crop_h = ((face_h_px * 0.22) as i32).max(8);

    let face_cx = (face.x * img_w as f64 + face.w * img_w as f64 * 0.5) as i32;
    let face_y_bottom = (face.y * img_h as f64 + face_h_px * 0.78) as i32;

    let mut x = face_cx - crop_w / 2;
    let mut y = face_y_bottom;
    let mut w = crop_w;
    let mut h = crop_h;

    // Clip against each image edge. Then clamp to positive dimensions
    // and enforce x+w <= img_w / y+h <= img_h via a second pass so a
    // face bbox that runs off the bottom doesn't produce an out-of-bounds
    // crop (happens when e.g. a forehead-only face is mis-detected).
    if x < 0 {
        w += x;
        x = 0;
    }
    if y < 0 {
        h += y;
        y = 0;
    }
    if x >= img_w as i32 {
        x = img_w as i32 - 1;
    }
    if y >= img_h as i32 {
        y = img_h as i32 - 1;
    }
    if x + w > img_w as i32 {
        w = img_w as i32 - x;
    }
    if y + h > img_h as i32 {
        h = img_h as i32 - y;
    }
    PixelCrop {
        x: x.max(0) as u32,
        y: y.max(0) as u32,
        w: w.max(1) as u32,
        h: h.max(1) as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mouth_crop_in_face_lower_third() {
        let face = NormBox {
            x: 0.3,
            y: 0.2,
            w: 0.3,
            h: 0.4,
        };
        let crop = mouth_crop_pixels(&face, 1000, 1000);
        // Face bbox runs y = 200..600. Mouth should sit below the
        // 0.78 mark → y ≈ 512.
        assert!(crop.y > 500 && crop.y < 540, "mouth y={}", crop.y);
        // Width ≈ 60% of 300px = 180.
        assert!((crop.w as i32 - 180).abs() <= 2, "mouth w={}", crop.w);
    }

    #[test]
    fn test_mouth_crop_respects_image_bounds() {
        // Face at image edge — crop must not extend past (img_w, img_h).
        let face = NormBox {
            x: 0.9,
            y: 0.85,
            w: 0.2,
            h: 0.2,
        };
        let crop = mouth_crop_pixels(&face, 1000, 1000);
        assert!(crop.x + crop.w <= 1000, "crop x+w={}", crop.x + crop.w);
        assert!(crop.y + crop.h <= 1000, "crop y+h={}", crop.y + crop.h);
    }

    #[test]
    fn test_mock_alternates() {
        let p = MockMouthProvider::default();
        let img = GrayImage::new(8, 8);
        let a = p.classify(&img).unwrap();
        let b = p.classify(&img).unwrap();
        assert_ne!(a.mouth_open, b.mouth_open, "mock must alternate");
    }
}
