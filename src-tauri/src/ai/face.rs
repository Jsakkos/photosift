use crate::ai::eye::{NormBox, NormPoint};
use anyhow::Result;
use image::GrayImage;

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
