use crate::ai::eye::EyeStateProvider;
use anyhow::{anyhow, Result};
use image::GrayImage;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use std::path::Path;
use std::sync::Mutex;

/// Input geometry. Matches the PINTO0309/OCEC family (MIT-licensed
/// sigmoid eye-open classifiers on GitHub) which is the first model we
/// verified against. Other models with different geometry (24×24,
/// 32×32 square) need a local adjustment here or a second provider.
const INPUT_H: usize = 24;
const INPUT_W: usize = 40;
const INPUT_C: usize = 3; // Replicated grayscale → 3 channels so RGB-trained models accept it.

/// ONNX-backed eye-state classifier. Verified against PINTO0309/OCEC;
/// other open-source options with the same 24×40 RGB + sigmoid
/// contract should drop in without change.
///
/// Drop-in contract (first model verified: OCEC):
/// - Input: f32 tensor `[1, 3, 24, 40]` in `[0, 1]`, 3 replicated
///   channels of grayscale so RGB-trained networks accept it without
///   colour fidelity we don't have. Input tensor name is read from
///   `Session::inputs()[0]` at load time (OCEC uses `images`).
/// - Output: first tensor, either
///     - shape `[1, 1]` / `[1]` sigmoid ("prob_open" > 0.5 = open), or
///     - shape `[1, 2]` / `[2]` argmax with index 1 = open.
pub struct OnnxEyeProvider {
    session: Mutex<Session>,
    input_name: String,
}

impl OnnxEyeProvider {
    pub fn load(model_path: &Path) -> Result<Self> {
        let session = Session::builder()
            .map_err(|e| anyhow!("ort builder: {}", e))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow!("ort opt level: {}", e))?
            .commit_from_file(model_path)
            .map_err(|e| anyhow!("ort load {}: {}", model_path.display(), e))?;
        let input_name = session
            .inputs()
            .first()
            .map(|o| o.name().to_string())
            .unwrap_or_else(|| "input".to_string());
        log::info!(
            "Eye classifier loaded from {} (input='{}')",
            model_path.display(),
            input_name
        );
        Ok(Self {
            session: Mutex::new(session),
            input_name,
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

    /// Package a grayscale eye crop into the NCHW float buffer the model
    /// expects. Resize uses box-sampling thumbnail (fast, stable quality
    /// at these tiny sizes) and the gray channel is replicated into R/G/B.
    fn prepare_input(crop: &GrayImage) -> Vec<f32> {
        let small = image::imageops::thumbnail(crop, INPUT_W as u32, INPUT_H as u32);
        let plane = INPUT_H * INPUT_W;
        let mut data = vec![0.0_f32; INPUT_C * plane];
        for (i, p) in small.pixels().enumerate() {
            let v = p.0[0] as f32 / 255.0;
            for c in 0..INPUT_C {
                data[c * plane + i] = v;
            }
        }
        data
    }
}

impl EyeStateProvider for OnnxEyeProvider {
    fn classify(&self, crop: &GrayImage) -> Result<i32> {
        let data = Self::prepare_input(crop);
        let tensor = Tensor::from_array((
            vec![1_i64, INPUT_C as i64, INPUT_H as i64, INPUT_W as i64],
            data,
        ))
        .map_err(|e| anyhow!("eye tensor build: {}", e))?;

        let mut guard = self.session.lock().unwrap_or_else(|p| p.into_inner());
        let name = self.input_name.as_str();
        let outputs = guard
            .run(ort::inputs![name => tensor])
            .map_err(|e| anyhow!("eye session run: {}", e))?;
        let (_shape, values) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| anyhow!("eye extract output: {}", e))?;
        Self::decide(&values.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Luma};

    #[test]
    fn test_decide_single_sigmoid() {
        assert_eq!(OnnxEyeProvider::decide(&[0.9]).unwrap(), 1);
        assert_eq!(OnnxEyeProvider::decide(&[0.2]).unwrap(), 0);
        assert_eq!(OnnxEyeProvider::decide(&[0.5]).unwrap(), 0);
    }

    #[test]
    fn test_decide_two_class_argmax() {
        assert_eq!(OnnxEyeProvider::decide(&[0.1, 0.9]).unwrap(), 1);
        assert_eq!(OnnxEyeProvider::decide(&[1.2, -0.3]).unwrap(), 0);
    }

    #[test]
    fn test_decide_rejects_weird_shapes() {
        assert!(OnnxEyeProvider::decide(&[]).is_err());
        assert!(OnnxEyeProvider::decide(&[1.0; 5]).is_err());
    }

    #[test]
    fn test_prepare_input_shape_and_replication() {
        let img: GrayImage = ImageBuffer::from_fn(48, 28, |_, _| Luma([128]));
        let data = OnnxEyeProvider::prepare_input(&img);
        assert_eq!(data.len(), INPUT_C * INPUT_H * INPUT_W);
        // All three channels should hold the same replicated value
        // (grayscale promoted to RGB).
        let plane = INPUT_H * INPUT_W;
        for i in 0..plane {
            assert!((data[i] - data[plane + i]).abs() < 1e-6);
            assert!((data[i] - data[2 * plane + i]).abs() < 1e-6);
        }
        // 128/255 ≈ 0.5019
        assert!((data[0] - (128.0 / 255.0)).abs() < 1e-4);
    }
}
