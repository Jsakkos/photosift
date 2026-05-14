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

/// Which eye open/closed classifier is in use. `Absent` means no
/// classifier is loaded — the AI worker skips eye classification for
/// every face and writes NULL into `left_eye_open` / `right_eye_open`.
/// The frontend gates eye indicators and the eye term of the AI-pick
/// score on `Onnx` so the absent state surfaces as "no data" rather
/// than silently scoring as zero-eyes-open.
///
/// Mock providers existed historically to keep the app booting when
/// the optional `eye_state.onnx` model wasn't on disk. Those wrote
/// alternating 0/1 noise that looked real but wasn't. Removed in favor
/// of honest NULL.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EyeProviderKind {
    Absent,
    Onnx,
}

/// Which mouth/smile classifier is in use. Mirrors `EyeProviderKind`.
/// `Absent` means no `mouth_state.onnx` on disk; the worker writes
/// NULL into `smile_score` and the UI hides smile indicators.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MouthProviderKind {
    Absent,
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
/// Only YuNet (face detection) is bundled. The eye-state and
/// mouth-state classifiers are user-supplied drops; the worker writes
/// NULL into the corresponding face columns when they're absent. See
/// `EyeProviderKind` for the rationale (no mock fallbacks).
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
    // `None` for `faces_provider` disables face detection entirely
    // (no faces written, no eye/smile classification possible) but
    // sharpness analysis still runs. `None` for eye / mouth / cat
    // skips that specific classifier; faces still get detected and
    // logged with NULL for the missing fields.
    faces_provider: Option<Box<dyn FaceProvider>>,
    eyes_provider: Option<Box<dyn EyeStateProvider>>,
    mouth_provider: Option<Box<dyn MouthStateProvider>>,
    cat_provider: Option<Box<dyn crate::ai::cat::CatDetectorProvider>>,
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
