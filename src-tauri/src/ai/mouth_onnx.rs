use crate::ai::mouth::{MouthState, MouthStateProvider};
use anyhow::{anyhow, Result};
use image::GrayImage;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::{Tensor, ValueType};
use std::path::Path;
use std::sync::Mutex;

/// Fallback geometry when the model doesn't declare a fixed input shape.
/// `32` matches the original OCMC-style stub; real FER+ and FER ONNXes
/// declare `64` explicitly and will be picked up dynamically at load time.
const DEFAULT_SIDE: usize = 32;

/// ONNX-backed mouth / smile classifier. Handles several shapes so we
/// can drop in any of these without per-model code changes:
///
/// **Input** (`[1, 1, H, W]` grayscale f32; H×W read from the session at
/// load time — e.g. 32 for mock-size mouth nets, 64 for FER+):
/// - input tensor name is read from the session; common names are `input`,
///   `Input3`, `images`.
/// - **Scale** depends on input size: FER/FER+ (≥48 px) were trained on
///   raw `[0, 255]` floats, while the OCMC-style small mouth stub used
///   `[0, 1]`. We branch on `input_h >= 48` so the right preprocessing
///   fires without per-model config.
///
/// **Output** (first tensor, any of):
/// - `[1]` / `[1, 1]` — sigmoid open/closed, smile fixed at 0.5 (unknown).
/// - `[2]` / `[1, 2]` — open/closed argmax, smile fixed at 0.5.
/// - `[3]` / `[1, 3]` — `[closed, open_neutral, smile]`, real smile.
/// - `[7]` / `[1, 7]` — FER (pre-FER+) logits
///   `[angry, disgust, fear, happy, sad, surprise, neutral]`.
/// - `[8]` / `[1, 8]` — FER+ logits
///   `[neutral, happiness, surprise, sadness, anger, disgust, fear, contempt]`.
///   Happy index is 1 post-softmax; mouth_open ≈ argmax != neutral.
pub struct OnnxMouthProvider {
    session: Mutex<Session>,
    input_name: String,
    input_h: usize,
    input_w: usize,
}

/// FER+ class order ([ONNX model zoo `emotion-ferplus-8`]). Index of happy = 1.
const FER_PLUS_HAPPY_IDX: usize = 1;
const FER_PLUS_NEUTRAL_IDX: usize = 0;

/// FER (7-class, pre-FER+) class order used by some older face-emotion
/// ONNXes. Happy sits at index 3, neutral at index 6.
const FER_HAPPY_IDX: usize = 3;
const FER_NEUTRAL_IDX: usize = 6;

impl OnnxMouthProvider {
    pub fn load(model_path: &Path) -> Result<Self> {
        let session = Session::builder()
            .map_err(|e| anyhow!("ort builder: {}", e))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow!("ort opt level: {}", e))?
            .commit_from_file(model_path)
            .map_err(|e| anyhow!("ort load {}: {}", model_path.display(), e))?;

        let input = session
            .inputs()
            .first()
            .ok_or_else(|| anyhow!("mouth model has no inputs"))?;
        let input_name = input.name().to_string();
        let (input_h, input_w) = match input.dtype() {
            ValueType::Tensor { shape, .. } if shape.len() == 4 => {
                // NCHW. Dims may be -1 (dynamic) if the model was exported
                // without concrete sizes — fall back to DEFAULT_SIDE then.
                let h = if shape[2] > 0 { shape[2] as usize } else { DEFAULT_SIDE };
                let w = if shape[3] > 0 { shape[3] as usize } else { DEFAULT_SIDE };
                (h, w)
            }
            _ => (DEFAULT_SIDE, DEFAULT_SIDE),
        };

        log::info!(
            "Mouth classifier loaded from {} (input='{}', {}×{})",
            model_path.display(),
            input_name,
            input_h,
            input_w
        );
        Ok(Self {
            session: Mutex::new(session),
            input_name,
            input_h,
            input_w,
        })
    }

    /// Numerically stable softmax. Returns a Vec of the same length.
    fn softmax(logits: &[f32]) -> Vec<f32> {
        let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let exps: Vec<f32> = logits.iter().map(|v| (v - max).exp()).collect();
        let sum: f32 = exps.iter().sum();
        if sum > 0.0 {
            exps.into_iter().map(|v| v / sum).collect()
        } else {
            vec![0.0; logits.len()]
        }
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
            7 => {
                // FER 7-class. Happy at index 3, neutral at index 6.
                let probs = Self::softmax(output);
                let smile = probs[FER_HAPPY_IDX] as f64;
                // Argmax — mouth_open = non-neutral dominant class.
                let argmax = probs
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|(i, _)| i)
                    .unwrap_or(FER_NEUTRAL_IDX);
                Ok(MouthState {
                    mouth_open: if argmax != FER_NEUTRAL_IDX { 1 } else { 0 },
                    smile_confidence: smile.clamp(0.0, 1.0),
                })
            }
            8 => {
                // FER+ 8-class: [neutral, happiness, surprise, sadness,
                //                anger, disgust, fear, contempt].
                // Any non-neutral dominant class counts as "mouth open";
                // smile_confidence is happy-class probability.
                let probs = Self::softmax(output);
                let smile = probs[FER_PLUS_HAPPY_IDX] as f64;
                let argmax = probs
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|(i, _)| i)
                    .unwrap_or(FER_PLUS_NEUTRAL_IDX);
                Ok(MouthState {
                    mouth_open: if argmax != FER_PLUS_NEUTRAL_IDX { 1 } else { 0 },
                    smile_confidence: smile.clamp(0.0, 1.0),
                })
            }
            n => Err(anyhow!(
                "mouth classifier returned {n} values; expected 1, 2, 3 (open/smile), 7 (FER), or 8 (FER+)"
            )),
        }
    }
}

impl MouthStateProvider for OnnxMouthProvider {
    fn classify(&self, crop: &GrayImage) -> Result<MouthState> {
        let h = self.input_h;
        let w = self.input_w;
        let small = image::imageops::thumbnail(crop, w as u32, h as u32);
        // FER/FER+ were trained on [0, 255] uint8→float inputs; small
        // mouth-specific nets (32×32) used [0, 1]. See type-level doc.
        let use_raw_scale = h >= 48 || w >= 48;
        let mut data = vec![0.0_f32; h * w];
        for (i, p) in small.pixels().enumerate() {
            data[i] = if use_raw_scale {
                p.0[0] as f32
            } else {
                p.0[0] as f32 / 255.0
            };
        }
        let tensor = Tensor::from_array((vec![1_i64, 1, h as i64, w as i64], data))
            .map_err(|e| anyhow!("mouth tensor build: {}", e))?;

        let mut guard = self.session.lock().unwrap_or_else(|p| p.into_inner());
        let name = self.input_name.as_str();
        let outputs = guard
            .run(ort::inputs![name => tensor])
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
        let s = OnnxMouthProvider::decide(&[0.9, 0.2]).unwrap();
        assert_eq!(s.mouth_open, 0);
    }

    #[test]
    fn test_decide_three_class_smile() {
        // [closed, open_neutral, smile] — smile wins.
        let s = OnnxMouthProvider::decide(&[0.1, 0.3, 0.7]).unwrap();
        assert_eq!(s.mouth_open, 1);
        assert!(s.smile_confidence > 0.5);
    }

    #[test]
    fn test_decide_fer_plus_happy_wins() {
        // FER+: [neutral, happy, surprise, sad, anger, disgust, fear, contempt]
        // Big logit on index 1 → happy dominates, smile_confidence ~ 1.
        let logits = [0.1, 5.0, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1];
        let s = OnnxMouthProvider::decide(&logits).unwrap();
        assert_eq!(s.mouth_open, 1, "happy != neutral, so mouth considered open");
        assert!(
            s.smile_confidence > 0.9,
            "softmax should concentrate mass on happy, got {}",
            s.smile_confidence
        );
    }

    #[test]
    fn test_decide_fer_plus_neutral_suppresses_smile() {
        // Neutral dominates — smile low, mouth not open.
        let logits = [5.0, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1];
        let s = OnnxMouthProvider::decide(&logits).unwrap();
        assert_eq!(s.mouth_open, 0);
        assert!(s.smile_confidence < 0.1);
    }

    #[test]
    fn test_decide_fer_seven_class_happy() {
        // FER: [angry, disgust, fear, happy, sad, surprise, neutral]
        let logits = [0.0, 0.0, 0.0, 4.0, 0.0, 0.0, 0.0];
        let s = OnnxMouthProvider::decide(&logits).unwrap();
        assert_eq!(s.mouth_open, 1, "happy argmax is non-neutral");
        assert!(s.smile_confidence > 0.8);
    }

    #[test]
    fn test_decide_rejects_weird_shapes() {
        assert!(OnnxMouthProvider::decide(&[]).is_err());
        assert!(OnnxMouthProvider::decide(&[1.0; 5]).is_err()); // 5 not supported
        assert!(OnnxMouthProvider::decide(&[1.0; 10]).is_err());
    }

    #[test]
    fn test_softmax_sums_to_one() {
        let probs = OnnxMouthProvider::softmax(&[1.0, 2.0, 3.0]);
        let sum: f32 = probs.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5);
        assert!(probs[2] > probs[1] && probs[1] > probs[0]);
    }
}
