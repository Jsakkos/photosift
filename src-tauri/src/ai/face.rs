use crate::ai::eye::{NormBox, NormPoint};
use crate::ai::AiProviderStatus;
use anyhow::{anyhow, Result};
use image::GrayImage;
use ort::session::{builder::GraphOptimizationLevel, Session};
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct DetectedFace {
    pub bbox: NormBox,
    pub left_eye: NormPoint,
    pub right_eye: NormPoint,
    pub confidence: f64,
}

pub trait FaceProvider: Send + Sync {
    fn detect(&self, gray: &GrayImage) -> Result<Vec<DetectedFace>>;
}

/// Real face detector backed by the YuNet 2023mar ONNX model.
///
/// The load path (model extraction → ORT session init → CUDA/CPU EP
/// selection) is wired end-to-end. The `detect` body currently returns
/// an empty vec — the anchor-decode output parser is deferred to a
/// follow-up task (tracked in docs/phase2-ai-qa.md). The pipeline
/// stays functional with no faces reported; mock fixtures continue to
/// cover downstream code paths.
pub struct YuNetProvider {
    session: Mutex<Session>,
}

impl YuNetProvider {
    /// Load the ONNX model. Tries CUDA execution provider first when
    /// `try_cuda` is true, falls back to CPU on any error. Returns the
    /// provider along with the status that actually applied so the UI
    /// can reflect the real backend.
    pub fn load(model_path: &Path, try_cuda: bool) -> Result<(Self, AiProviderStatus)> {
        let (session, status) = if try_cuda {
            match Self::build_session(model_path, true) {
                Ok(s) => (s, AiProviderStatus::Cuda),
                Err(e) => {
                    log::warn!("YuNet CUDA load failed ({}); falling back to CPU", e);
                    (
                        Self::build_session(model_path, false)?,
                        AiProviderStatus::Cpu,
                    )
                }
            }
        } else {
            (
                Self::build_session(model_path, false)?,
                AiProviderStatus::Cpu,
            )
        };
        Ok((
            YuNetProvider {
                session: Mutex::new(session),
            },
            status,
        ))
    }

    fn build_session(model_path: &Path, cuda: bool) -> Result<Session> {
        let builder = Session::builder()
            .map_err(|e| anyhow!("ort Session::builder failed: {}", e))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow!("ort with_optimization_level failed: {}", e))?;
        let mut builder = if cuda {
            use ort::ep::CUDAExecutionProvider;
            builder
                .with_execution_providers([CUDAExecutionProvider::default().build()])
                .map_err(|e| anyhow!("ort with_execution_providers(cuda) failed: {}", e))?
        } else {
            use ort::ep::CPUExecutionProvider;
            builder
                .with_execution_providers([CPUExecutionProvider::default().build()])
                .map_err(|e| anyhow!("ort with_execution_providers(cpu) failed: {}", e))?
        };
        builder
            .commit_from_file(model_path)
            .map_err(|e| anyhow!("ort commit_from_file failed: {}", e))
    }
}

impl FaceProvider for YuNetProvider {
    fn detect(&self, _gray: &GrayImage) -> Result<Vec<DetectedFace>> {
        // Minimal viable implementation: load path is wired (CUDA→CPU
        // fallback, model-bundling, ORT linking) but the YuNet anchor
        // decoder is not yet implemented. Returning Ok(vec![]) keeps the
        // pipeline functional — sharpness/eye-state still run via the
        // mock eye provider, and downstream UI handles zero faces
        // gracefully. Real output decoding (per-stride heads 8/16/32,
        // confidence threshold 0.6, NMS IoU 0.3) lands in a follow-up.
        let _guard = match self.session.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        Ok(Vec::new())
    }
}

// Keep the eye geometry types visibly referenced in this file so future
// work on the output decoder can wire them without another import
// shuffle. Intentionally unused today.
#[allow(dead_code)]
fn _typecheck_helpers(_: NormBox, _: NormPoint) {}
