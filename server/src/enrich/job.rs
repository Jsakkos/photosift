//! The enrichment worker: one background thread that analyses every photo
//! whose `enriched_at` is NULL, shoot by shoot, and records faces, identity
//! embeddings and quality signals. The import job calls `EnrichWorker::enqueue`
//! for every shoot it touched.

use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use fast_image_resize::FilterType;
use rayon::prelude::*;

use super::faces::Detector;
use super::identity::Embedder;
use super::moments;
use super::quality::{self, gray_of};
use super::Backend;
use crate::config::Settings;
use crate::db::photos::{self, NewFace, PhotoRow, QualityColumns};
use crate::db::DbPool;
use crate::events::{EventHub, ServerEvent};
use crate::media::{
    embedded,
    render::{self, resize_with, RgbImage},
};

/// Global sharpness and exposure are measured at this long edge so scores are
/// comparable between photos and cheap to compute.
const ANALYSIS_EDGE: u32 = 1024;

/// What `GET /api/status` reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// `"cuda"` only when every loaded model runs on CUDA, `"cpu"` when any
    /// fell back, `"unavailable"` when the detector is missing.
    pub ai_backend: &'static str,
    /// False when the identity model is missing: faces carry no embedding.
    pub embeddings: bool,
    /// Shoots waiting for or undergoing enrichment.
    pub queue: usize,
}

struct Models {
    detector: Detector,
    /// Faces are still detected without it; they just carry no embedding.
    embedder: Option<Embedder>,
    /// Effective backend: CUDA only if every loaded model runs on it.
    backend: Backend,
}

#[derive(Default)]
struct State {
    pending: VecDeque<i64>,
    active: Option<i64>,
    shutdown: bool,
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    backend: Option<Backend>,
    embeddings: bool,
}

impl Shared {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Stops the worker thread when the last `EnrichWorker` clone goes away.
struct ShutdownOnDrop(Arc<Shared>);

impl Drop for ShutdownOnDrop {
    fn drop(&mut self) {
        self.0.lock().shutdown = true;
        self.0.wake.notify_all();
    }
}

/// Cheap to clone; all clones feed the same worker thread.
#[derive(Clone)]
pub struct EnrichWorker {
    shared: Arc<Shared>,
    _shutdown: Arc<ShutdownOnDrop>,
}

struct Ctx {
    db: DbPool,
    events: EventHub,
    cache_root: PathBuf,
    models: Option<Models>,
    settings: Settings,
}

impl EnrichWorker {
    /// Loads the models from `<data_root>/models` (their absence is not an
    /// error: photos still get quality signals), starts the thread, and queues
    /// any shoot left unenriched by an earlier run.
    pub fn start(db: DbPool, events: EventHub, data_root: &Path, settings: Settings) -> Self {
        let models = load_models(&data_root.join("models"));
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            wake: Condvar::new(),
            backend: models.as_ref().map(|m| m.backend),
            embeddings: models.as_ref().is_some_and(|m| m.embedder.is_some()),
        });
        let ctx = Ctx {
            db: db.clone(),
            events,
            cache_root: data_root.join("cache"),
            models,
            settings,
        };
        let thread_shared = Arc::clone(&shared);
        let spawned = std::thread::Builder::new()
            .name("enrich".into())
            .spawn(move || worker_loop(&thread_shared, &ctx));
        if let Err(e) = spawned {
            crate::plog!("enrich", "could not start the enrichment thread: {e}");
        }
        let worker = Self {
            _shutdown: Arc::new(ShutdownOnDrop(Arc::clone(&shared))),
            shared,
        };
        match photos::shoots_needing_enrichment(&db) {
            Ok(ids) => ids.into_iter().for_each(|id| worker.enqueue(id)),
            Err(e) => crate::plog!("enrich", "could not look for unenriched photos: {e:#}"),
        }
        worker
    }

    /// Queues a shoot. A shoot already waiting is not queued twice; one that is
    /// running is queued again so photos added meanwhile are picked up.
    pub fn enqueue(&self, shoot_id: i64) {
        let mut state = self.shared.lock();
        if !state.pending.contains(&shoot_id) {
            state.pending.push_back(shoot_id);
        }
        drop(state);
        self.shared.wake.notify_one();
    }

    pub fn status(&self) -> Status {
        let state = self.shared.lock();
        Status {
            ai_backend: self.shared.backend.map_or("unavailable", Backend::as_str),
            embeddings: self.shared.embeddings,
            queue: state.pending.len() + usize::from(state.active.is_some()),
        }
    }
}

fn load_models(dir: &Path) -> Option<Models> {
    let (detector, detector_backend) = match Detector::load(dir, true) {
        Ok(d) => d,
        Err(e) => {
            crate::plog!("enrich", "face detection unavailable: {e:#}");
            return None;
        }
    };
    let (embedder, backend) = match Embedder::load(dir, detector_backend == Backend::Cuda) {
        Ok((e, embedder_backend)) => {
            let both = if detector_backend == Backend::Cuda && embedder_backend == Backend::Cuda {
                Backend::Cuda
            } else {
                Backend::Cpu
            };
            (Some(e), both)
        }
        Err(e) => {
            crate::plog!("enrich", "face identity unavailable: {e:#}");
            (None, detector_backend)
        }
    };
    Some(Models {
        detector,
        embedder,
        backend,
    })
}

fn worker_loop(shared: &Shared, ctx: &Ctx) {
    loop {
        let shoot_id = {
            let mut state = shared.lock();
            loop {
                if state.shutdown {
                    return;
                }
                if let Some(id) = state.pending.pop_front() {
                    state.active = Some(id);
                    break id;
                }
                state = shared.wake.wait(state).unwrap_or_else(|e| e.into_inner());
            }
        };
        if catch_unwind(AssertUnwindSafe(|| run_shoot(ctx, shoot_id))).is_err() {
            crate::plog!("enrich", "enrichment of shoot {shoot_id} panicked");
            // A UI waiting on this shoot must not hang; its photos stay
            // unenriched and are retried at the next start.
            ctx.events.send(ServerEvent::EnrichDone { shoot_id });
        }
        shared.lock().active = None;
    }
}

fn run_shoot(ctx: &Ctx, shoot_id: i64) {
    let ids = match photos::unenriched_photo_ids(&ctx.db, shoot_id) {
        Ok(ids) => ids,
        Err(e) => {
            crate::plog!("enrich", "listing photos of shoot {shoot_id}: {e:#}");
            ctx.events.send(ServerEvent::EnrichDone { shoot_id });
            return;
        }
    };
    let total = ids.len();
    let done = AtomicUsize::new(0);
    let progress = |n: usize| {
        ctx.events.send(ServerEvent::EnrichProgress {
            shoot_id,
            done: n,
            total,
        });
    };
    progress(0);
    ids.par_iter().for_each(|&id| {
        // A bad file must not stall the shoot: record it as analysed-with-nothing.
        let analysed = catch_unwind(AssertUnwindSafe(|| enrich_photo(ctx, id)));
        if analysed.is_err() {
            crate::plog!("enrich", "enrichment of photo {id} panicked");
            save(ctx, id, &QualityColumns::default(), &[]);
        }
        progress(done.fetch_add(1, Ordering::SeqCst) + 1);
    });
    after_shoot_enriched(ctx, shoot_id);
    ctx.events.send(ServerEvent::EnrichDone { shoot_id });
}

/// Groups the shoot's photos into moments and bursts once every photo has its
/// signals. A failure is logged, never fatal: the photos stay reviewable, just
/// without sections.
fn after_shoot_enriched(ctx: &Ctx, shoot_id: i64) {
    if let Err(e) = moments::assign(&ctx.db, shoot_id, &ctx.settings) {
        crate::plog!("enrich", "grouping shoot {shoot_id} into moments: {e:#}");
    }
}

fn save(ctx: &Ctx, photo_id: i64, quality: &QualityColumns, faces: &[NewFace]) {
    if let Err(e) = photos::save_enrichment(&ctx.db, photo_id, quality, faces) {
        // Left unenriched, so the next start retries it.
        crate::plog!("enrich", "saving enrichment of photo {photo_id}: {e:#}");
    }
}

fn enrich_photo(ctx: &Ctx, id: i64) {
    let row = match photos::get_photo(&ctx.db, id) {
        Ok(Some(row)) => row,
        Ok(None) => return,
        Err(e) => {
            crate::plog!("enrich", "reading photo {id}: {e:#}");
            return;
        }
    };
    let Some(img) = load_preview(ctx, &row) else {
        crate::plog!(
            "enrich",
            "photo {id}: no usable preview; recording no signals"
        );
        save(ctx, id, &QualityColumns::default(), &[]);
        return;
    };
    let (faces, quality) = analyse(ctx.models.as_ref(), &img);
    save(ctx, id, &quality, &faces);
}

fn analyse(models: Option<&Models>, img: &RgbImage) -> (Vec<NewFace>, QualityColumns) {
    let mut faces = Vec::new();
    if let Some(models) = models {
        for f in models.detector.detect(img) {
            let embedding = models.embedder.as_ref().and_then(|e| {
                match e.embed(img, &f.landmarks_px(img.w, img.h)) {
                    Ok(v) => Some(v.to_vec()),
                    Err(err) => {
                        crate::plog!("enrich", "embedding failed: {err:#}");
                        None
                    }
                }
            });
            faces.push(NewFace {
                bbox: f.bbox,
                landmarks: f.landmarks,
                score: f.score,
                embedding,
                sharpness: quality::face_sharpness(img, &f),
            });
        }
    }
    let face_sharpness = faces.iter().filter_map(|f| f.sharpness).reduce(f64::max);

    let q = match small_gray(img) {
        Some((gray, w, h)) => quality::measure(&gray, w, h, &[]),
        None => return (faces, QualityColumns::default()),
    };
    (
        faces,
        QualityColumns {
            sharpness: Some(q.sharpness),
            face_sharpness,
            mean_luma: Some(q.mean_luma),
            clip_hi: Some(q.clip_hi),
            clip_lo: Some(q.clip_lo),
        },
    )
}

/// Grayscale of the frame at `ANALYSIS_EDGE` (never upscaled), box-filtered.
fn small_gray(img: &RgbImage) -> Option<(Vec<f32>, usize, usize)> {
    let long = img.w.max(img.h);
    if long <= ANALYSIS_EDGE {
        return Some((gray_of(img), img.w as usize, img.h as usize));
    }
    let s = f64::from(ANALYSIS_EDGE) / f64::from(long);
    let scaled = |v: u32| ((f64::from(v) * s).round() as u32).max(1);
    let small = resize_with(img, scaled(img.w), scaled(img.h), FilterType::Box).ok()?;
    Some((gray_of(&small), small.w as usize, small.h as usize))
}

fn preview_path(ctx: &Ctx, id: i64) -> PathBuf {
    ctx.cache_root.join("preview").join(format!("{id}.jpg"))
}

/// Decodes the cached preview, rebuilding it from the library file first when
/// it is missing or unreadable (a crash between recording a photo and writing
/// its preview leaves exactly that).
fn load_preview(ctx: &Ctx, row: &PhotoRow) -> Option<RgbImage> {
    let path = preview_path(ctx, row.id);
    let read = || {
        let bytes = std::fs::read(&path).ok()?;
        render::decode_jpeg(&bytes).ok()
    };
    if let Some(img) = read() {
        return Some(img);
    }
    if let Err(e) = regenerate_preview(ctx, row) {
        crate::plog!("enrich", "photo {}: cannot rebuild preview: {e:#}", row.id);
        return None;
    }
    read()
}

fn regenerate_preview(ctx: &Ctx, row: &PhotoRow) -> anyhow::Result<()> {
    let source = row
        .raw_path
        .as_deref()
        .or(row.jpeg_path.as_deref())
        .ok_or_else(|| anyhow::anyhow!("photo has no library file"))?;
    let source = Path::new(source);
    let bytes = std::fs::read(source)?;
    let ext = source
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default();
    let jpeg = embedded::largest_jpeg(&bytes, &ext)?;
    let orientation = row
        .orientation
        .and_then(|o| u16::try_from(o).ok())
        .unwrap_or(1);
    let thumb = ctx.cache_root.join("thumb").join(format!("{}.jpg", row.id));
    render::make_previews(jpeg, orientation, &preview_path(ctx, row.id), &thumb)?;
    Ok(())
}
