//! On-device enrichment of imported photos (faces, identity, quality, moments).

pub mod faces;
pub mod identity;
pub mod job;
pub mod quality;

use std::path::Path;

use anyhow::{anyhow, Result};
use ort::session::{builder::GraphOptimizationLevel, Session};

/// Where inference actually runs; reported by `GET /api/status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Cuda,
    Cpu,
}

impl Backend {
    pub fn as_str(self) -> &'static str {
        match self {
            Backend::Cuda => "cuda",
            Backend::Cpu => "cpu",
        }
    }
}

fn build_session(path: &Path, cuda: bool) -> Result<Session> {
    // ort's builder errors carry non-Send state, so stringify them for anyhow.
    let builder = Session::builder()
        .map_err(|e| anyhow!("{e}"))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| anyhow!("{e}"))?;
    let mut builder = if cuda {
        // error_on_failure: without it ORT silently drops to CPU at session
        // creation and the caller could not report the real backend.
        builder
            .with_execution_providers([ort::ep::CUDAExecutionProvider::default()
                .build()
                .error_on_failure()])
            .map_err(|e| anyhow!("{e}"))?
    } else {
        builder
            .with_execution_providers([ort::ep::CPUExecutionProvider::default().build()])
            .map_err(|e| anyhow!("{e}"))?
    };
    builder.commit_from_file(path).map_err(|e| anyhow!("{e}"))
}

/// Loads `path` on CUDA when asked and possible; otherwise on CPU. A CUDA
/// failure (missing DLLs, no GPU) is logged and never fatal.
pub(crate) fn load_session(path: &Path, cuda: bool) -> Result<(Session, Backend)> {
    if cuda {
        match build_session(path, true) {
            Ok(s) => return Ok((s, Backend::Cuda)),
            Err(e) => eprintln!(
                "photosift: CUDA unavailable for {} ({e:#}); using CPU",
                path.display()
            ),
        }
    }
    Ok((build_session(path, false)?, Backend::Cpu))
}
