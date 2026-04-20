use crate::ai::mouth::{MouthState, MouthStateProvider};
use anyhow::{anyhow, Result};
use image::GrayImage;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use std::path::Path;
use std::sync::Mutex;

const INPUT_SIDE: usize = 32;

/// ONNX-backed mouth classifier.
///
/// Contract for the bundled model (documented in reference memory;
/// adapt locally if your model differs):
/// - Input: first input tensor, shape `[1, 1, 32, 32]` f32 in `[0, 1]`.
/// - Output: first output tensor in one of two shapes:
///     - `[1, 1]` / `[1]`: sigmoid probability of mouth-open.
///     - `[1, 2]` / `[2]`: open/closed logits or softmax, index 1 = open.
///     - `[1, 3]` / `[3]`: richer model that emits
///       `[closed, open_neutral, smile]` — we combine open = argmax != 0
///       and smile_confidence = softmax(open_neutral, smile) on index 2.
///
/// Smile confidence falls back to `0.5` ("unknown") when the model is
/// open/closed-only so the UI can still render something consistent.
pub struct OnnxMouthProvider {
    session: Mutex<Session>,
}

impl OnnxMouthProvider {
    pub fn load(model_path: &Path) -> Result<Self> {
        let session = Session::builder()
            .map_err(|e| anyhow!("ort builder: {}", e))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow!("ort opt level: {}", e))?
            .commit_from_file(model_path)
            .map_err(|e| anyhow!("ort load {}: {}", model_path.display(), e))?;
        log::info!("Mouth classifier loaded from {}", model_path.display());
        Ok(Self {
            session: Mutex::new(session),
        })
    }

    fn decide(output: &[f32]) -> Result<MouthState> {
        match output.len() {
            1 => Ok(MouthState {
                mouth_open: if output[0] > 0.5 { 1 } else { 0 },
                smile_confidence: 0.5,
            }),
            2 => Ok(MouthState {
                mouth_open: if output[1] > output[0] { 1 } else { 0 },
                smile_confidence: 0.5,
            }),
            3 => {
                // [closed, open_neutral, smile]. Mouth open if either
                // "open_neutral" or "smile" class wins.
                let closed = output[0];
                let open_neutral = output[1];
                let smile = output[2];
                let mouth_open = if open_neutral.max(smile) > closed {
                    1
                } else {
                    0
                };
                // Smile confidence is the smile class's share of the
                // open-mouth mass, bounded 0..1.
                let denom = (open_neutral.max(0.0) + smile.max(0.0)).max(1e-6);
                let smile_conf = (smile.max(0.0) / denom).clamp(0.0, 1.0) as f64;
                Ok(MouthState {
                    mouth_open,
                    smile_confidence: smile_conf,
                })
            }
            n => Err(anyhow!(
                "mouth classifier returned {n} values; expected 1 (sigmoid), 2 (open/closed), or 3 (+smile)"
            )),
        }
    }
}

impl MouthStateProvider for OnnxMouthProvider {
    fn classify(&self, crop: &GrayImage) -> Result<MouthState> {
        let small = image::imageops::thumbnail(crop, INPUT_SIDE as u32, INPUT_SIDE as u32);
        let mut data = vec![0.0_f32; INPUT_SIDE * INPUT_SIDE];
        for (i, p) in small.pixels().enumerate() {
            data[i] = p.0[0] as f32 / 255.0;
        }
        let tensor = Tensor::from_array((
            vec![1_i64, 1, INPUT_SIDE as i64, INPUT_SIDE as i64],
            data,
        ))
        .map_err(|e| anyhow!("mouth tensor build: {}", e))?;

        let mut guard = self.session.lock().unwrap_or_else(|p| p.into_inner());
        let outputs = guard
            .run(ort::inputs!["input" => tensor])
            .map_err(|e| anyhow!("mouth session run: {}", e))?;
        let (_shape, values) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow!("mouth extract output: {}", e))?;
        Self::decide(&values.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decide_sigmoid_open() {
        let s = OnnxMouthProvider::decide(&[0.8]).unwrap();
        assert_eq!(s.mouth_open, 1);
        assert!((s.smile_confidence - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_decide_two_class_closed() {
        // [closed, open] logits — closed wins.
        let s = OnnxMouthProvider::decide(&[0.9, 0.2]).unwrap();
        assert_eq!(s.mouth_open, 0);
    }

    #[test]
    fn test_decide_three_class_smile() {
        // [closed, open_neutral, smile] — smile wins, so mouth open and
        // smile confidence > open_neutral share.
        let s = OnnxMouthProvider::decide(&[0.1, 0.3, 0.7]).unwrap();
        assert_eq!(s.mouth_open, 1);
        assert!(s.smile_confidence > 0.5);
    }

    #[test]
    fn test_decide_rejects_weird_shapes() {
        assert!(OnnxMouthProvider::decide(&[]).is_err());
        assert!(OnnxMouthProvider::decide(&[1.0; 10]).is_err());
    }
}
