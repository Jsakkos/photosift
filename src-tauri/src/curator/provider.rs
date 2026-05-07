//! Provider-neutral interface for the Curator. Concrete impls live in
//! `api_anthropic.rs`, `api_gemini.rs`, `api_local.rs`. The worker holds
//! an `Arc<dyn CuratorProvider>` and never sees provider details.
//!
//! Mirrors the trait pattern in `crate::ai` (`FaceProvider`,
//! `EyeStateProvider`, `MouthStateProvider`): selection happens once at
//! worker spawn from the user's `curator_provider` setting.

use crate::curator::types::{CuratorJudgment, ShootSummary, Usage};
use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;

/// One frame fed into a Stage 2 cluster. Borrowed so callers can build
/// these without copying thumbnail paths.
pub struct Stage2Frame<'a> {
    pub photo_id: i64,
    pub thumb_path: &'a Path,
    pub sharpness_1_10: Option<i32>,
    pub face_count: Option<i32>,
    pub eyes_open_count: Option<i32>,
    pub smile_score: Option<f64>,
}

pub struct Stage2Cluster<'a> {
    pub frames: &'a [Stage2Frame<'a>],
}

/// Result of a Stage 1 (shoot summary) call.
#[derive(Debug)]
pub struct SummaryResult {
    pub summary: ShootSummary,
    pub usage: Usage,
    pub model: String,
}

/// Result of a Stage 2 (one cluster's worth of judgments) call.
#[derive(Debug)]
pub struct ClusterJudgmentBatch {
    pub judgments: Vec<CuratorJudgment>,
    pub usage: Usage,
    pub model: String,
    pub prompt_version: i32,
}

/// Behaviour that every Curator provider must implement. Object-safe so
/// we can store one as `Arc<dyn CuratorProvider>` in the worker.
#[async_trait]
pub trait CuratorProvider: Send + Sync {
    /// Cheap probe used by the Settings "Test connection" button.
    async fn test_connection(&self) -> Result<()>;

    /// Stage 1 — characterize the shoot from a stratified thumbnail
    /// sample. Returns a `ShootSummary` to inline in Stage 2 prompts.
    async fn run_stage1(
        &self,
        photo_count: i64,
        cluster_count: i64,
        thumb_paths: &[&Path],
    ) -> Result<SummaryResult>;

    /// Stage 2 — judge one cluster (or a batch of singletons) given the
    /// Stage 1 summary as context.
    async fn run_stage2_cluster(
        &self,
        summary: &ShootSummary,
        cluster: &Stage2Cluster<'_>,
    ) -> Result<ClusterJudgmentBatch>;

    /// Stable identifier persisted to `curator_judgments.provider`.
    /// "anthropic" | "gemini" | "local".
    fn provider_id(&self) -> &'static str;

    /// Model identifier currently configured. Cost tracking reads it
    /// from each call's `ClusterJudgmentBatch.model` directly, but this
    /// accessor is handy for diagnostics and future progress events.
    #[allow(dead_code)]
    fn model(&self) -> &str;
}
