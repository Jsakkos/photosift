pub mod cat;
pub mod face;
pub mod eye;
pub mod eye_onnx;
pub mod mouth;
pub mod mouth_onnx;
pub mod sharpness;
pub mod worker;
pub mod mock;

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AiProviderStatus {
    Cuda,
    Cpu,
    Disabled,
}

/// Which eye open/closed classifier is in use. `Mock` means deterministic
/// alternating 0/1 — not a real signal. The frontend checks this to hide
/// eye indicators and drop the `(1 + eyes_open)` weighting from the
/// AI-pick score so mock values don't corrupt ranking.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EyeProviderKind {
    Mock,
    Onnx,
}

/// Which mouth/smile classifier is in use. Mirrors `EyeProviderKind`.
/// UI must gate mouth indicators on `Onnx` so the mock's alternating
/// output doesn't surface to users.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MouthProviderKind {
    Mock,
    Onnx,
}

#[derive(Debug, Clone)]
pub struct AiJob {
    pub shoot_id: i64,
    pub photo_id: i64,
    pub preview_path: String,
}

pub use worker::{process_job, run_loop, WorkerHandle};

const YUNET_BYTES: &[u8] = include_bytes!("models/yunet.onnx");

/// Extract bundled ONNX models to ~/.photosift/models/ on first run.
/// Currently only YuNet is bundled; eye-state classifier is mock-backed
/// pending model sourcing (see docs/phase2-ai-qa.md).
pub fn ensure_models_on_disk() -> anyhow::Result<std::path::PathBuf> {
    let dir = crate::db::schema::photosift_home().join("models");
    std::fs::create_dir_all(&dir)?;
    let yunet = dir.join("yunet.onnx");
    if !yunet.exists() {
        std::fs::write(&yunet, YUNET_BYTES)?;
    }
    Ok(dir)
}

use crate::ai::eye::EyeStateProvider;
use crate::ai::face::FaceProvider;
use crate::ai::mouth::MouthStateProvider;
use crate::db::schema::Database;
use crossbeam_channel::unbounded;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

pub struct SpawnedWorker {
    pub handle: worker::WorkerHandle,
}

/// Spawn the AI background worker. Opens its own Database handle against
/// `db_path` so it owns the connection exclusively (SQLite with WAL handles
/// the reader/writer overlap). Returns the handle immediately; job
/// processing happens on the spawned thread.
pub fn spawn_worker(
    db_path: std::path::PathBuf,
    faces_provider: Box<dyn FaceProvider>,
    eyes_provider: Box<dyn EyeStateProvider>,
    mouth_provider: Box<dyn MouthStateProvider>,
    cat_provider: Box<dyn crate::ai::cat::CatDetectorProvider>,
    cancel: Arc<AtomicBool>,
    analyzed: Arc<AtomicUsize>,
    failed: Arc<AtomicUsize>,
    on_job_done: impl Fn(i64, bool) + Send + 'static,
) -> SpawnedWorker {
    let (tx, rx) = unbounded::<AiJob>();
    let cancel_clone = cancel.clone();
    thread::spawn(move || {
        let db = match Database::open(&db_path) {
            Ok(d) => d,
            Err(e) => {
                log::error!("ai worker db open failed: {}", e);
                return;
            }
        };
        worker::run_loop(
            rx,
            cancel_clone,
            db,
            faces_provider,
            eyes_provider,
            mouth_provider,
            cat_provider,
            move |job, res| match res {
                Ok(_) => {
                    analyzed.fetch_add(1, Ordering::SeqCst);
                    on_job_done(job.photo_id, true);
                }
                Err(e) => {
                    // {:#} on anyhow::Error shows the full context chain
                    // so the underlying cause (decode failure, missing
                    // file, etc.) shows up in the log, not just "open
                    // preview PATH".
                    log::error!("ai job failed for photo {}: {:#}", job.photo_id, e);
                    failed.fetch_add(1, Ordering::SeqCst);
                    on_job_done(job.photo_id, false);
                }
            },
        );
    });
    SpawnedWorker {
        handle: worker::WorkerHandle { sender: tx, cancel },
    }
}
