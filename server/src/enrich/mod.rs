//! On-device enrichment of imported photos (faces, identity, quality, moments).

pub mod faces;
pub mod identity;
pub mod job;
pub mod moments;
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

/// Loads `path` on CUDA when asked and possible; otherwise on CPU. `warm_up`
/// runs one inference on the new session, inside the fallback: a CUDA session
/// that builds but cannot run (cuDNN/runtime errors surface on first run) is
/// logged and retried on CPU, never fatal.
pub(crate) fn load_session(
    path: &Path,
    cuda: bool,
    warm_up: impl Fn(&mut Session) -> Result<()>,
) -> Result<(Session, Backend)> {
    with_cpu_fallback(cuda, |on_cuda| {
        let mut session = build_session(path, on_cuda)?;
        warm_up(&mut session)?;
        Ok(session)
    })
    .map_err(|e| e.context(path.display().to_string()))
}

/// Tries `attempt(true)` when `cuda` is set, and `attempt(false)` if that
/// fails or CUDA was not asked for.
fn with_cpu_fallback<T>(cuda: bool, attempt: impl Fn(bool) -> Result<T>) -> Result<(T, Backend)> {
    if cuda {
        match attempt(true) {
            Ok(v) => return Ok((v, Backend::Cuda)),
            Err(e) => crate::plog!("enrich", "CUDA unavailable ({e:#}); using CPU"),
        }
    }
    Ok((attempt(false)?, Backend::Cpu))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn cuda_failure_after_the_session_builds_retries_on_cpu() {
        // Models the warm-up run failing on CUDA after the session was built.
        let tried = Cell::new(Vec::new());
        let (v, backend) = with_cpu_fallback(true, |cuda| {
            let mut t = tried.take();
            t.push(cuda);
            tried.set(t);
            if cuda {
                Err(anyhow!("cudnn runtime error"))
            } else {
                Ok("cpu session")
            }
        })
        .unwrap();
        assert_eq!((v, backend), ("cpu session", Backend::Cpu));
        assert_eq!(tried.take(), vec![true, false]);
    }

    #[test]
    fn working_cuda_is_kept_and_cpu_only_never_tries_cuda() {
        let (_, backend) = with_cpu_fallback(true, |_| Ok(())).unwrap();
        assert_eq!(backend, Backend::Cuda);
        let (_, backend) = with_cpu_fallback(false, |cuda| {
            assert!(!cuda);
            Ok(())
        })
        .unwrap();
        assert_eq!(backend, Backend::Cpu);
    }

    #[test]
    fn failure_on_cpu_is_an_error() {
        assert!(with_cpu_fallback::<()>(true, |_| Err(anyhow!("broken"))).is_err());
    }
}
