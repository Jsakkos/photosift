use crate::ai::eye::{EyeStateProvider, NormBox, NormPoint};
use crate::ai::face::{DetectedFace, FaceProvider};
use anyhow::Result;
use image::GrayImage;
use std::sync::atomic::{AtomicI32, Ordering};

/// Test double: returns preset canned faces + a deterministic alternating
/// open/closed classification.
pub struct MockFaceProvider {
    pub fixed_faces: Vec<DetectedFace>,
}

impl Default for MockFaceProvider {
    fn default() -> Self {
        Self {
            fixed_faces: vec![DetectedFace {
                bbox: NormBox { x: 0.3, y: 0.2, w: 0.3, h: 0.4 },
                left_eye: NormPoint { x: 0.38, y: 0.32 },
                right_eye: NormPoint { x: 0.52, y: 0.32 },
                confidence: 0.9,
            }],
        }
    }
}

impl FaceProvider for MockFaceProvider {
    fn detect(&self, _: &GrayImage) -> Result<Vec<DetectedFace>> {
        Ok(self.fixed_faces.clone())
    }
}

pub struct MockEyeProvider {
    counter: AtomicI32,
}

impl Default for MockEyeProvider {
    fn default() -> Self {
        Self { counter: AtomicI32::new(0) }
    }
}

impl EyeStateProvider for MockEyeProvider {
    fn classify(&self, _: &GrayImage) -> Result<i32> {
        let n = self.counter.fetch_add(1, Ordering::SeqCst);
        Ok(n % 2)
    }
}
