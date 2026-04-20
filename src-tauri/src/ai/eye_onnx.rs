use crate::ai::eye::EyeStateProvider;
use anyhow::{anyhow, Result};
use image::GrayImage;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use std::path::Path;
use std::sync::Mutex;

/// Input square edge length in pixels. 24 is the de-facto default for
/// CEW-trained MobileNet-v2-ish eye-state classifiers that sit on PyPI
/// / HuggingFace. If a model expects 32 or 64, adjust here — the rest
/// of the pipeline is tolerant because we thumbnail-resize into the
/// target anyway.
const INPUT_SIDE: usize = 24;

/// ONNX-backed eye-state classifier.
///
/// Contract the bundled model is expected to meet:
/// - Input: first input tensor, shape `[1, 1, H, W]` or `[1, H, W, 1]`
///   f32, grayscale with values in `[0, 1]`.
/// - Output: first output tensor, either
///     - shape `[1, 1]` / `[1]` with a sigmoid-activated probability
///       (value > 0.5 means open), or
///     - shape `[1, 2]` / `[2]` with two scores where index 1 = open
///       (works for both logits and softmax outputs — we argmax).
///
/// If your model doesn't match, either convert it with `onnx.compose`
/// utilities or rename the provider to something model-specific.
pub struct OnnxEyeProvider {
    session: Mutex<Session>,
}

impl OnnxEyeProvider {
    pub fn load(model_path: &Path) -> Result<Self> {
        let session = Session::builder()
            .map_err(|e| anyhow!("ort builder: {}", e))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow!("ort opt level: {}", e))?
            .commit_from_file(model_path)
            .map_err(|e| anyhow!("ort load {}: {}", model_path.display(), e))?;
        log::info!("Eye classifier loaded from {}", model_path.display());
        Ok(Self {
            session: Mutex::new(session),
        })
    }

    fn decide(output: &[f32]) -> Result<i32> {
        match output.len() {
            1 => Ok(if output[0] > 0.5 { 1 } else { 0 }),
            2 => Ok(if output[1] > output[0] { 1 } else { 0 }),
            n => Err(anyhow!(
                "eye classifier returned {n} values; expected 1 (sigmoid) or 2 (softmax/logits)"
            )),
        }
    }
}

impl EyeStateProvider for OnnxEyeProvider {
    fn classify(&self, crop: &GrayImage) -> Result<i32> {
        // Downscale the eye crop to the model's input size via
        // box-sample thumbnail — fast, stable quality at these tiny
        // sizes, and avoids the quality filter's high ratio overhead.
        let small = image::imageops::thumbnail(crop, INPUT_SIDE as u32, INPUT_SIDE as u32);
        let mut data = vec![0.0_f32; INPUT_SIDE * INPUT_SIDE];
        for (i, p) in small.pixels().enumerate() {
            data[i] = p.0[0] as f32 / 255.0;
        }

        let tensor = Tensor::from_array((
            vec![1_i64, 1, INPUT_SIDE as i64, INPUT_SIDE as i64],
            data,
        ))
        .map_err(|e| anyhow!("eye tensor build: {}", e))?;

        // Models from different sources use different input tensor names
        // ("input", "input_1", "data", "images", …). ORT's run() lets us
        // pass an unnamed input as positional; fall back to "input" via
        // the named inputs macro which ort's convention uses by default.
        let mut guard = self.session.lock().unwrap_or_else(|p| p.into_inner());
        let outputs = guard
            .run(ort::inputs!["input" => tensor])
            .map_err(|e| anyhow!("eye session run: {}", e))?;

        let (_shape, values) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow!("eye extract output: {}", e))?;
        let vec_values: Vec<f32> = values.to_vec();
        Self::decide(&vec_values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decide_single_sigmoid() {
        assert_eq!(OnnxEyeProvider::decide(&[0.9]).unwrap(), 1);
        assert_eq!(OnnxEyeProvider::decide(&[0.2]).unwrap(), 0);
        assert_eq!(OnnxEyeProvider::decide(&[0.5]).unwrap(), 0);
    }

    #[test]
    fn test_decide_two_class_argmax() {
        // [closed_logit, open_logit] — index 1 wins when the model
        // calls the eye open.
        assert_eq!(OnnxEyeProvider::decide(&[0.1, 0.9]).unwrap(), 1);
        assert_eq!(OnnxEyeProvider::decide(&[1.2, -0.3]).unwrap(), 0);
    }

    #[test]
    fn test_decide_rejects_weird_shapes() {
        assert!(OnnxEyeProvider::decide(&[]).is_err());
        assert!(OnnxEyeProvider::decide(&[1.0; 5]).is_err());
    }
}
