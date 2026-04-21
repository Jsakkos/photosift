//! Cat face / body detector. Mirrors the shape of `FaceProvider` so the
//! worker can run it alongside YuNet without special-casing species.
//!
//! **Status (2026-04-20):** the trait, mock, and persistence path are
//! live. No ONNX provider is wired yet — that's a follow-up pending a
//! specific cat detector model (YOLOv8-cat-face, YOLOX-pets, or similar).
//! The worker calls the mock today, which always returns empty. Dropping
//! in a real detector is a focused change: implement `CatDetectorProvider`
//! with the new ONNX class, swap `MockCatDetector` for it in `lib.rs` when
//! `~/.photosift/models/cat_detector.onnx` exists.
//!
//! Bbox values returned from the provider are **normalized** (0.0–1.0
//! fractions of image width/height) so downstream code matches what
//! YuNet already produces.

use anyhow::Result;
use image::RgbImage;

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
