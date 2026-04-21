//! Cat face / body detector. Mirrors the shape of `FaceProvider` so the
//! worker can run it alongside YuNet without special-casing species.
//!
//! **Status (2026-04-20):** the trait, mock, persistence path, and an
//! `OnnxCatDetector` backed by Tiny-YOLOv3 (ONNX Model Zoo) are live.
//! YOLOv3 detects cat *bodies*, not cat *faces*, so the bbox covers the
//! whole cat rather than just the head — good enough for "photo contains
//! a cat" signal but per-cat-face quality scoring would need a dedicated
//! cat-face model.
//!
//! Bbox values returned from the provider are **normalized** (0.0–1.0
//! fractions of image width/height) so downstream code matches what
//! YuNet already produces.

use anyhow::{anyhow, Result};
use image::RgbImage;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::{Tensor, ValueType};
use std::path::Path;
use std::sync::Mutex;

use crate::ai::eye::NormBox;

/// A single detected cat. `confidence` is the model's own score (typically
/// post-sigmoid / post-softmax); the worker persists it as
/// `detection_confidence` on the row. Eye landmarks are `None` because no
/// widely-available cat face model surfaces them — the pipeline skips eye
/// classification for cat rows and leaves `left_eye_open`/`right_eye_open`
/// at 0 (closed), which the UI hides behind the species gate.
#[derive(Debug, Clone)]
pub struct DetectedCat {
    pub bbox: NormBox,
    pub confidence: f64,
}

pub trait CatDetectorProvider: Send + Sync {
    fn detect(&self, rgb: &RgbImage) -> Result<Vec<DetectedCat>>;
}

/// Stand-in provider used until a real ONNX detector lands. Always
/// returns no cats — keeps the pipeline quiet and the schema populated
/// with only human faces until a real model is wired.
pub struct MockCatDetector;

impl Default for MockCatDetector {
    fn default() -> Self {
        Self
    }
}

impl CatDetectorProvider for MockCatDetector {
    fn detect(&self, _rgb: &RgbImage) -> Result<Vec<DetectedCat>> {
        Ok(Vec::new())
    }
}

/// COCO class index for "cat". Tiny-YOLOv3 inherits the COCO class list
/// unchanged; the index is stable across YOLO variants trained on COCO.
const COCO_CAT_CLASS: i64 = 15;

/// Tiny-YOLOv3 was trained with 416×416 letterboxed inputs + gray (128)
/// padding. The ONNX export accepts dynamic dims in principle, but the
/// anchor geometry is sized for 416 so stepping away from it breaks
/// recall quickly — we match training.
const YOLO_INPUT_SIDE: usize = 416;

/// Minimum per-detection score we bother persisting. Tiny-YOLOv3's NMS
/// layer already filters at a coarser threshold internally, but real-world
/// cat photos still emit noisy ~20 % detections on furniture we'd rather
/// not store. Tuned so Bengals on a carpet still register while a
/// polka-dot blanket doesn't.
const MIN_SCORE: f32 = 0.30;

/// ONNX-backed cat detector using Tiny-YOLOv3 from the ONNX Model Zoo.
///
/// Model contract (`tiny-yolov3-11.onnx`):
/// - Inputs:
///     - `input_1` shape `[1, 3, 416, 416]` f32 in `[0, 1]`, HWC→NCHW,
///       letterboxed with gray (128, 128, 128) padding to preserve
///       aspect ratio.
///     - `image_shape` shape `[1, 2]` f32, the *original* `[H, W]` in
///       pixels. The model uses this to reverse the letterbox when
///       reporting box coordinates, so output boxes are already in the
///       original image's pixel space.
/// - Outputs (NMS is baked into the graph):
///     - `yolonms_layer_1/ExpandDims_1:0` shape `[1, N, 4]`, boxes in
///       `(y1, x1, y2, x2)` original-image coords.
///     - `yolonms_layer_1/ExpandDims_3:0` shape `[1, 80, N]`, per-class
///       scores in COCO order.
///     - `yolonms_layer_1/concat_2:0` shape `[M, 3]`, `(batch, class, box)`
///       triples for the surviving detections.
pub struct OnnxCatDetector {
    session: Mutex<Session>,
    input_name: String,
    shape_input_name: String,
}

impl OnnxCatDetector {
    pub fn load(model_path: &Path) -> Result<Self> {
        let session = Session::builder()
            .map_err(|e| anyhow!("ort builder: {}", e))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow!("ort opt level: {}", e))?
            .commit_from_file(model_path)
            .map_err(|e| anyhow!("ort load {}: {}", model_path.display(), e))?;

        let inputs = session.inputs();
        // Tiny-YOLOv3 has exactly two inputs: the image tensor, then the
        // original-shape tensor. Identify each by its tensor rank since
        // name conventions vary between exporters (`input_1` / `images`).
        let mut input_name: Option<String> = None;
        let mut shape_input_name: Option<String> = None;
        for inp in inputs {
            match inp.dtype() {
                ValueType::Tensor { shape, .. } if shape.len() == 4 => {
                    input_name = Some(inp.name().to_string());
                }
                ValueType::Tensor { shape, .. } if shape.len() == 2 => {
                    shape_input_name = Some(inp.name().to_string());
                }
                _ => {}
            }
        }
        let input_name = input_name
            .ok_or_else(|| anyhow!("cat detector: no 4-D image input found"))?;
        let shape_input_name = shape_input_name
            .ok_or_else(|| anyhow!("cat detector: no 2-D image-shape input found"))?;

        log::info!(
            "Cat detector loaded from {} (inputs: '{}', '{}')",
            model_path.display(),
            input_name,
            shape_input_name
        );
        Ok(Self {
            session: Mutex::new(session),
            input_name,
            shape_input_name,
        })
    }

    /// Letterbox an RGB image into a 416×416 buffer with gray fill,
    /// preserving aspect ratio. Returns the NCHW 0–1 float tensor data.
    fn preprocess(rgb: &RgbImage) -> Vec<f32> {
        let (iw, ih) = rgb.dimensions();
        let side = YOLO_INPUT_SIDE as u32;
        // Scale to fit inside side×side, preserving aspect.
        let scale = (side as f64 / iw as f64).min(side as f64 / ih as f64);
        let nw = ((iw as f64 * scale).round() as u32).max(1).min(side);
        let nh = ((ih as f64 * scale).round() as u32).max(1).min(side);

        let resized = image::imageops::resize(rgb, nw, nh, image::imageops::FilterType::Triangle);

        // Pad: start with gray 128, then paste the resized image centered.
        let pad_x = (side - nw) / 2;
        let pad_y = (side - nh) / 2;

        let plane = (side as usize) * (side as usize);
        let mut data = vec![128.0_f32 / 255.0; 3 * plane];

        for y in 0..nh {
            for x in 0..nw {
                let pixel = resized.get_pixel(x, y);
                let dst_idx = ((pad_y + y) * side + (pad_x + x)) as usize;
                data[dst_idx] = pixel[0] as f32 / 255.0;
                data[plane + dst_idx] = pixel[1] as f32 / 255.0;
                data[2 * plane + dst_idx] = pixel[2] as f32 / 255.0;
            }
        }
        data
    }
}

impl CatDetectorProvider for OnnxCatDetector {
    fn detect(&self, rgb: &RgbImage) -> Result<Vec<DetectedCat>> {
        let (img_w, img_h) = rgb.dimensions();
        let data = Self::preprocess(rgb);
        let side = YOLO_INPUT_SIDE as i64;
        let image_tensor = Tensor::from_array((vec![1_i64, 3, side, side], data))
            .map_err(|e| anyhow!("cat image tensor build: {}", e))?;
        // image_shape carries the *original* (H, W) so the model can reverse
        // the letterbox when emitting boxes. Order matters here.
        let shape_tensor = Tensor::from_array((vec![1_i64, 2], vec![img_h as f32, img_w as f32]))
            .map_err(|e| anyhow!("cat shape tensor build: {}", e))?;

        let image_name = self.input_name.as_str();
        let shape_name = self.shape_input_name.as_str();

        let mut guard = self.session.lock().unwrap_or_else(|p| p.into_inner());
        let outputs = guard
            .run(ort::inputs![image_name => image_tensor, shape_name => shape_tensor])
            .map_err(|e| anyhow!("cat session run: {}", e))?;

        // Outputs are returned in definition order by the graph:
        //   [0] = boxes      [1, N, 4]   (y1, x1, y2, x2)
        //   [1] = scores     [1, 80, N]
        //   [2] = indices    [M, 3]      (batch, class, box)
        let (boxes_shape, boxes) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow!("cat extract boxes: {}", e))?;
        let (_scores_shape, scores) = outputs[1]
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow!("cat extract scores: {}", e))?;
        let (indices_shape, indices) = outputs[2]
            .try_extract_tensor::<i32>()
            .map_err(|e| anyhow!("cat extract indices: {}", e))?;

        // Trust the *actual* slice lengths rather than the reported shape
        // dims — Tiny-YOLOv3's NMS layer emits dynamic shapes that come
        // back as `-1` through ort, which then wraps into a nonsense
        // `usize` if you use it directly. `boxes` is `[1, N, 4]` so the
        // N we need is boxes.len() / 4; `indices` is `[M, 3]` so M is
        // indices.len() / 3.
        let _ = boxes_shape;
        let _ = indices_shape;
        let num_boxes = boxes.len() / 4;
        let num_selected = indices.len() / 3;

        let mut detections = Vec::new();
        for i in 0..num_selected {
            let cls = indices[i * 3 + 1] as i64;
            if cls != COCO_CAT_CLASS {
                continue;
            }
            let box_idx = indices[i * 3 + 2] as usize;
            if box_idx >= num_boxes {
                continue;
            }
            // scores is [1, 80, N]: index with (cls * N) + box.
            let score_idx = (cls as usize) * num_boxes + box_idx;
            if score_idx >= scores.len() {
                continue;
            }
            let score = scores[score_idx];
            if !score.is_finite() || score < MIN_SCORE {
                continue;
            }

            // boxes is [1, N, 4] packed, y1 x1 y2 x2 in original px.
            let base = box_idx * 4;
            if base + 3 >= boxes.len() {
                continue;
            }
            let y1 = boxes[base].max(0.0);
            let x1 = boxes[base + 1].max(0.0);
            let y2 = boxes[base + 2].min(img_h as f32);
            let x2 = boxes[base + 3].min(img_w as f32);
            if y2 <= y1 || x2 <= x1 {
                continue;
            }
            let bbox = NormBox {
                x: (x1 as f64) / (img_w as f64),
                y: (y1 as f64) / (img_h as f64),
                w: ((x2 - x1) as f64) / (img_w as f64),
                h: ((y2 - y1) as f64) / (img_h as f64),
            };
            detections.push(DetectedCat {
                bbox,
                confidence: score as f64,
            });
        }

        Ok(detections)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preprocess_shape_and_fill() {
        let img: RgbImage = image::ImageBuffer::from_fn(100, 50, |_, _| image::Rgb([255, 0, 0]));
        let data = OnnxCatDetector::preprocess(&img);
        let plane = YOLO_INPUT_SIDE * YOLO_INPUT_SIDE;
        assert_eq!(data.len(), 3 * plane);
        // Gray fill shows up at 128/255 in every channel at the edges where
        // letterbox padding lives.
        let gray = 128.0_f32 / 255.0;
        // Corners (outside the scaled content) are gray.
        assert!((data[0] - gray).abs() < 1e-5);
        assert!((data[plane] - gray).abs() < 1e-5);
        assert!((data[2 * plane] - gray).abs() < 1e-5);
    }
}
