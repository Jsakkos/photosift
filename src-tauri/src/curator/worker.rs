//! Curator background worker. Runs on its own dedicated OS thread that
//! hosts a single-threaded tokio runtime. The worker drains a tokio
//! mpsc queue, dispatches Stage 1 + Stage 2 jobs against the Anthropic
//! API with bounded concurrency, persists each result to SQLite, and
//! emits Tauri events as progress is made.
//!
//! Mirror of `ai/worker.rs` in shape (dedicated thread, owns its own
//! `Database` handle) but uses tokio + reqwest internally because the
//! workload is HTTP-bound, not CPU-bound. The two workers are
//! independent — an Anthropic outage never stalls face detection.

use crate::curator::cost::actual_cost_cents_for_usage;
use crate::curator::provider::{
    ClusterJudgmentBatch, CuratorProvider, Stage2Cluster, Stage2Frame,
};
use crate::curator::types::ShootSummary;
use crate::db::schema::Database;
use anyhow::{Context, Result};
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, Semaphore};

#[derive(Debug, Clone)]
pub enum CuratorJob {
    /// Build the shoot summary (Stage 1). Stage 2 jobs are enqueued
    /// once Stage 1 returns.
    Stage1 { shoot_id: i64 },
    /// One phash cluster, evaluated against the cached Stage 1 prefix.
    Stage2Cluster { shoot_id: i64, group_id: i64 },
    /// Up to ~6 ungrouped photos, batched into one Stage 2 call. The
    /// Stage 2 prompt is unchanged but the model is told to leave
    /// `cluster_rank` null.
    Stage2Singletons { shoot_id: i64, photo_ids: Vec<i64> },
    /// A batch of photos for the on-import triage stage. Independent of
    /// Stage 1 / groups — needs neither. Writes `triage_judgments` rows
    /// and emits `curator:triage_done`.
    TriageBatch { shoot_id: i64, photo_ids: Vec<i64> },
}

/// Photos per `TriageBatch` job — one. Triage sends a single image per LLM
/// call: vision models judge one frame reliably, but rush and misattribute
/// when handed many at once (a 50-image batch labelled a sharp couch shot
/// "grossly out of focus"). One image per call also makes the verdict
/// correct by construction — there is no other frame to confuse it with.
/// The system prompt is cached, so per-call overhead stays small.
pub const TRIAGE_BATCH_SIZE: usize = 1;

// Stage 2 concurrency is per-provider — cloud=4, local=1. See
// `CuratorProvider::concurrency_limit()` and consumers in `run_loop`.

/// Max ungrouped photos per `Stage2Singletons` job.
const SINGLETONS_PER_JOB: usize = 6;

#[derive(Clone)]
pub struct WorkerHandle {
    pub sender: mpsc::UnboundedSender<CuratorJob>,
    pub cancel: Arc<AtomicBool>,
}

#[derive(Clone, Default)]
pub struct ProgressCounters {
    pub processed: Arc<AtomicUsize>,
    pub failed: Arc<AtomicUsize>,
    pub total: Arc<AtomicUsize>,
    pub cost_cents: Arc<AtomicUsize>,
}

/// Spawn the worker on a dedicated OS thread. Returns a handle for
/// enqueueing jobs and cancelling. The worker owns its own Database
/// handle (rusqlite + WAL handles the reader/writer overlap with the
/// app's main DB connection).
#[allow(clippy::too_many_arguments)]
pub fn spawn(
    db_path: PathBuf,
    provider: Arc<dyn CuratorProvider>,
    cancel: Arc<AtomicBool>,
    progress: ProgressCounters,
    cost_cap_cents: u32,
    app: AppHandle,
) -> WorkerHandle {
    let (tx, rx) = mpsc::unbounded_channel::<CuratorJob>();
    let cancel_clone = cancel.clone();
    let tx_for_self_enqueue = tx.clone();

    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                log::error!("curator worker: build tokio rt failed: {}", e);
                return;
            }
        };

        let db = match Database::open(&db_path) {
            Ok(d) => d,
            Err(e) => {
                log::error!("curator worker db open failed: {}", e);
                return;
            }
        };

        rt.block_on(run_loop(
            rx,
            tx_for_self_enqueue,
            cancel_clone,
            provider,
            db,
            progress,
            cost_cap_cents,
            app,
        ));
    });

    WorkerHandle { sender: tx, cancel }
}

#[allow(clippy::too_many_arguments)]
async fn run_loop(
    mut rx: mpsc::UnboundedReceiver<CuratorJob>,
    self_tx: mpsc::UnboundedSender<CuratorJob>,
    cancel: Arc<AtomicBool>,
    provider: Arc<dyn CuratorProvider>,
    db: Database,
    progress: ProgressCounters,
    cost_cap_cents: u32,
    app: AppHandle,
) {
    // The provider is held behind an Arc so each spawned Stage 2 task
    // gets a cheap clone of the same instance (reqwest::Client inside
    // each impl is itself Arc'd).
    let concurrency = provider.concurrency_limit().max(1);
    let semaphore = Arc::new(Semaphore::new(concurrency));

    while let Some(job) = rx.recv().await {
        if cancel.load(Ordering::SeqCst) {
            log::info!("curator worker: cancel flag set, draining queue");
            // Drain remaining queued jobs without dispatching.
            while rx.try_recv().is_ok() {}
            continue;
        }
        if cost_cap_cents > 0
            && progress.cost_cents.load(Ordering::SeqCst) as u32 >= cost_cap_cents
        {
            log::warn!(
                "curator worker: cost cap {}c reached; dropping job {:?}",
                cost_cap_cents,
                job
            );
            // Per the plan: in-flight calls finish, no new dispatches.
            // Drain queue without sending more.
            while rx.try_recv().is_ok() {}
            continue;
        }

        match job {
            CuratorJob::Stage1 { shoot_id } => {
                if let Err(e) = handle_stage1(
                    provider.as_ref(),
                    &db,
                    shoot_id,
                    &progress,
                    &self_tx,
                    &app,
                )
                .await
                {
                    log::error!("curator stage1 failed for shoot {}: {:#}", shoot_id, e);
                    progress.failed.fetch_add(1, Ordering::SeqCst);
                    let _ = app.emit(
                        "curator:failed",
                        json!({ "shootId": shoot_id, "reason": format!("stage1: {}", e) }),
                    );
                }
            }
            CuratorJob::Stage2Cluster { shoot_id, group_id } => {
                let permit = semaphore.clone().acquire_owned().await.ok();
                tokio::spawn(handle_stage2_cluster(
                    permit,
                    Arc::clone(&provider),
                    db_path_from_db(&db),
                    shoot_id,
                    group_id,
                    progress.clone(),
                    app.clone(),
                ));
            }
            CuratorJob::Stage2Singletons { shoot_id, photo_ids } => {
                let permit = semaphore.clone().acquire_owned().await.ok();
                tokio::spawn(handle_stage2_singletons(
                    permit,
                    Arc::clone(&provider),
                    db_path_from_db(&db),
                    shoot_id,
                    photo_ids,
                    progress.clone(),
                    app.clone(),
                ));
            }
            CuratorJob::TriageBatch { shoot_id, photo_ids } => {
                let permit = semaphore.clone().acquire_owned().await.ok();
                tokio::spawn(handle_triage_batch(
                    permit,
                    Arc::clone(&provider),
                    db_path_from_db(&db),
                    shoot_id,
                    photo_ids,
                    progress.clone(),
                    app.clone(),
                ));
            }
        }
    }

    // Channel was closed (sender dropped). Give in-flight tasks a
    // moment to settle before tearing down.
    let _ = tokio::time::timeout(
        Duration::from_secs(60),
        wait_until_idle(&semaphore, concurrency),
    )
    .await;
}

/// Wait until all permits are returned to the semaphore (i.e. no in-flight
/// Stage 2 tasks remain).
async fn wait_until_idle(sem: &Arc<Semaphore>, concurrency: usize) {
    while sem.available_permits() < concurrency {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Recover the DB file path from a Database for reopening in spawned
/// tokio tasks. Each task opens its own connection so SQLite never sees
/// concurrent writes through the same connection. WAL handles the
/// reader/writer overlap.
fn db_path_from_db(_db: &Database) -> PathBuf {
    // We don't store the path on Database, so fall back to the global
    // path. Phase B always uses the global DB; if multiple databases
    // ever come into play, plumb the path through the spawn args.
    crate::db::schema::global_db_path()
}

async fn handle_stage1(
    provider: &dyn CuratorProvider,
    db: &Database,
    shoot_id: i64,
    progress: &ProgressCounters,
    self_tx: &mpsc::UnboundedSender<CuratorJob>,
    app: &AppHandle,
) -> Result<()> {
    log::info!("curator stage1 starting for shoot {}", shoot_id);

    // Plan stage 2 work first so we can know cluster_count up front and
    // sample thumbnails stratified across clusters.
    let groups = db
        .get_groups_for_shoot(shoot_id)
        .context("get_groups_for_shoot")?;
    let mut group_member_ids: Vec<Vec<i64>> = groups
        .iter()
        .map(|g| g.members.iter().map(|m| m.photo_id).collect::<Vec<i64>>())
        .collect();
    let grouped_set: std::collections::HashSet<i64> = group_member_ids
        .iter()
        .flatten()
        .copied()
        .collect();
    let all_photo_ids = db
        .photos_for_shoot(shoot_id)
        .context("photos_for_shoot")?
        .into_iter()
        .map(|p| p.id)
        .collect::<Vec<i64>>();
    let singletons: Vec<i64> = all_photo_ids
        .iter()
        .copied()
        .filter(|id| !grouped_set.contains(id))
        .collect();
    let total_photos = all_photo_ids.len() as i64;
    let cluster_count = group_member_ids.len() as i64;

    // Stratified sample: take up to 30 thumbnails, one per cluster
    // (largest first). If fewer than 30 clusters, fill the remainder
    // from singletons.
    let mut sample: Vec<i64> = Vec::with_capacity(30);
    group_member_ids.sort_by(|a, b| b.len().cmp(&a.len()));
    for grp in &group_member_ids {
        if sample.len() >= 30 {
            break;
        }
        if let Some(&id) = grp.first() {
            sample.push(id);
        }
    }
    for &id in &singletons {
        if sample.len() >= 30 {
            break;
        }
        sample.push(id);
    }

    let thumb_dir = crate::db::schema::shoot_cache_dir(shoot_id).join("thumbs");
    let thumb_paths: Vec<PathBuf> = sample
        .iter()
        .map(|id| thumb_dir.join(format!("{}.jpg", id)))
        .filter(|p| p.exists())
        .collect();

    if thumb_paths.is_empty() {
        anyhow::bail!("no thumbnails on disk for shoot {}", shoot_id);
    }

    let thumb_refs: Vec<&std::path::Path> = thumb_paths.iter().map(|p| p.as_path()).collect();
    let result = provider
        .run_stage1(total_photos, cluster_count, &thumb_refs)
        .await?;

    // Persist Stage 1 summary + cost.
    let summary_json = serde_json::to_string(&result.summary).context("serialize summary")?;
    db.set_curator_summary(shoot_id, &summary_json)?;
    let stage1_cents =
        actual_cost_cents_for_usage(provider.provider_id(), &result.model, &result.usage);
    progress.cost_cents.fetch_add(stage1_cents as usize, Ordering::SeqCst);
    db.add_curator_cost_cents(shoot_id, stage1_cents)?;

    // Total = stage 1 (counted) + one stage-2 job per cluster + ceil(singletons/N).
    let singleton_jobs = singletons.len().div_ceil(SINGLETONS_PER_JOB);
    let stage2_jobs = group_member_ids.len() + singleton_jobs;
    progress
        .total
        .fetch_add(1 + stage2_jobs, Ordering::SeqCst);
    progress.processed.fetch_add(1, Ordering::SeqCst);

    let _ = app.emit(
        "curator:progress",
        json!({
            "shootId": shoot_id,
            "processed": progress.processed.load(Ordering::SeqCst),
            "total": progress.total.load(Ordering::SeqCst),
            "costCents": progress.cost_cents.load(Ordering::SeqCst),
            "stage": "stage1_done",
        }),
    );

    // Enqueue Stage 2 jobs. Clusters first (bigger, more interesting),
    // singletons after — gives the user signal in Triage faster.
    for grp in &groups {
        let _ = self_tx.send(CuratorJob::Stage2Cluster {
            shoot_id,
            group_id: grp.id,
        });
    }
    for chunk in singletons.chunks(SINGLETONS_PER_JOB) {
        let _ = self_tx.send(CuratorJob::Stage2Singletons {
            shoot_id,
            photo_ids: chunk.to_vec(),
        });
    }

    Ok(())
}

async fn handle_stage2_cluster(
    _permit: Option<tokio::sync::OwnedSemaphorePermit>,
    provider: Arc<dyn CuratorProvider>,
    db_path: PathBuf,
    shoot_id: i64,
    group_id: i64,
    progress: ProgressCounters,
    app: AppHandle,
) {
    let provider_id = provider.provider_id();
    let result = run_stage2_cluster(provider.as_ref(), &db_path, shoot_id, group_id).await;
    finalize_stage2(result, provider_id, shoot_id, Some(group_id), progress, app);
}

async fn handle_stage2_singletons(
    _permit: Option<tokio::sync::OwnedSemaphorePermit>,
    provider: Arc<dyn CuratorProvider>,
    db_path: PathBuf,
    shoot_id: i64,
    photo_ids: Vec<i64>,
    progress: ProgressCounters,
    app: AppHandle,
) {
    let provider_id = provider.provider_id();
    let result = run_stage2_singletons(provider.as_ref(), &db_path, shoot_id, &photo_ids).await;
    finalize_stage2(result, provider_id, shoot_id, None, progress, app);
}

fn finalize_stage2(
    result: Result<(ClusterJudgmentBatch, Vec<i64>)>,
    provider_id: &'static str,
    shoot_id: i64,
    group_id: Option<i64>,
    progress: ProgressCounters,
    app: AppHandle,
) {
    match result {
        Ok((batch, photo_ids)) => {
            // Persist judgments + cost in the worker thread; that's
            // what owns the Database handle in Phase B.
            let cents = actual_cost_cents_for_usage(provider_id, &batch.model, &batch.usage);
            progress.cost_cents.fetch_add(cents as usize, Ordering::SeqCst);
            // Use a fresh DB handle for this task.
            if let Ok(db) = Database::open(&crate::db::schema::global_db_path()) {
                for j in &batch.judgments {
                    let _ = db.upsert_curator_judgment(
                        j.photo_id,
                        shoot_id,
                        j.composition,
                        j.aesthetic,
                        j.cluster_rank,
                        j.is_keeper,
                        j.suggested_flag.as_str(),
                        &j.reason,
                        provider_id,
                        &batch.model,
                        batch.prompt_version,
                    );
                }
                // Express the cluster ranking as a tournament bracket so
                // the Review tab can show the Curator's pairwise picks
                // alongside the user's own. Derived, not re-prompted —
                // each judgment's `reason` is the rationale. Only real
                // clusters (group_id present) get a bracket.
                if let Some(gid) = group_id {
                    let ranked_ids = rank_judgments(&batch.judgments);
                    if ranked_ids.len() >= 2 {
                        let _ = db.delete_bracket_decisions_for_group(gid, "curator");
                        for (round, pair, left, right, decision) in
                            derive_curator_bracket(&ranked_ids)
                        {
                            let _ = db.insert_bracket_decision(
                                shoot_id, gid, round, pair, left, right, decision,
                                "curator",
                            );
                        }
                    }
                }
                let _ = db.add_curator_cost_cents(shoot_id, cents);

                // If everything is now judged, mark complete.
                if let Ok(remaining) = db.photos_needing_curator(shoot_id) {
                    if remaining.is_empty() {
                        let _ = db.mark_curator_completed(shoot_id);
                        let _ = app.emit(
                            "curator:completed",
                            json!({ "shootId": shoot_id }),
                        );
                    }
                }
            }
            progress.processed.fetch_add(1, Ordering::SeqCst);

            // Sanity: the model should return one judgment per frame.
            // If not, log it.
            if batch.judgments.len() != photo_ids.len() {
                log::warn!(
                    "curator stage2: model returned {} judgments for {} frames (shoot {}, group {:?})",
                    batch.judgments.len(),
                    photo_ids.len(),
                    shoot_id,
                    group_id
                );
            }

            let _ = app.emit(
                "curator:cluster_done",
                json!({
                    "shootId": shoot_id,
                    "groupId": group_id,
                    "processed": progress.processed.load(Ordering::SeqCst),
                    "total": progress.total.load(Ordering::SeqCst),
                    "costCents": progress.cost_cents.load(Ordering::SeqCst),
                }),
            );
        }
        Err(e) => {
            progress.failed.fetch_add(1, Ordering::SeqCst);
            progress.processed.fetch_add(1, Ordering::SeqCst);
            log::error!(
                "curator stage2 failed (shoot {}, group {:?}): {:#}",
                shoot_id,
                group_id,
                e
            );
            let _ = app.emit(
                "curator:failed",
                json!({
                    "shootId": shoot_id,
                    "groupId": group_id,
                    "reason": format!("{}", e),
                }),
            );
        }
    }

    // Always emit a per-task progress event so the UI bar moves.
    let _ = app.emit(
        "curator:progress",
        json!({
            "shootId": shoot_id,
            "processed": progress.processed.load(Ordering::SeqCst),
            "total": progress.total.load(Ordering::SeqCst),
            "costCents": progress.cost_cents.load(Ordering::SeqCst),
        }),
    );
}

async fn run_stage2_cluster(
    provider: &dyn CuratorProvider,
    db_path: &std::path::Path,
    shoot_id: i64,
    group_id: i64,
) -> Result<(ClusterJudgmentBatch, Vec<i64>)> {
    let db = Database::open(db_path).context("open db in stage2 task")?;
    let summary_json = db
        .curator_summary_json(shoot_id)
        .context("read curator_summary")?
        .ok_or_else(|| anyhow::anyhow!("no curator_summary for shoot {}", shoot_id))?;
    let summary: ShootSummary =
        serde_json::from_str(&summary_json).context("parse curator_summary json")?;

    let groups = db.get_groups_for_shoot(shoot_id)?;
    let group = groups
        .into_iter()
        .find(|g| g.id == group_id)
        .ok_or_else(|| anyhow::anyhow!("group {} not found", group_id))?;

    let photo_ids: Vec<i64> = group.members.iter().map(|m| m.photo_id).collect();
    let frames = build_stage2_frames(&db, shoot_id, &photo_ids)?;
    let frame_refs: Vec<Stage2Frame<'_>> = frames
        .iter()
        .map(|f| Stage2Frame {
            photo_id: f.photo_id,
            thumb_path: &f.thumb_path,
            sharpness_1_10: f.sharpness_1_10,
            face_count: f.face_count,
            eyes_open_count: f.eyes_open_count,
            smile_score: f.smile_score,
        })
        .collect();
    let cluster = Stage2Cluster { frames: &frame_refs };
    let batch = provider.run_stage2_cluster(&summary, &cluster).await?;
    coerce_singleton_to_pick_keep(batch, false, photo_ids).map(|(b, ids)| (b, ids))
}

async fn run_stage2_singletons(
    provider: &dyn CuratorProvider,
    db_path: &std::path::Path,
    shoot_id: i64,
    photo_ids: &[i64],
) -> Result<(ClusterJudgmentBatch, Vec<i64>)> {
    let db = Database::open(db_path).context("open db in singletons task")?;
    let summary_json = db
        .curator_summary_json(shoot_id)
        .context("read curator_summary")?
        .ok_or_else(|| anyhow::anyhow!("no curator_summary for shoot {}", shoot_id))?;
    let summary: ShootSummary = serde_json::from_str(&summary_json)?;

    let frames = build_stage2_frames(&db, shoot_id, photo_ids)?;
    let frame_refs: Vec<Stage2Frame<'_>> = frames
        .iter()
        .map(|f| Stage2Frame {
            photo_id: f.photo_id,
            thumb_path: &f.thumb_path,
            sharpness_1_10: f.sharpness_1_10,
            face_count: f.face_count,
            eyes_open_count: f.eyes_open_count,
            smile_score: f.smile_score,
        })
        .collect();
    let cluster = Stage2Cluster { frames: &frame_refs };
    let mut batch = provider.run_stage2_cluster(&summary, &cluster).await?;
    // Force null cluster_rank for singletons: even if the model emits
    // a rank, it has no cluster context.
    for j in &mut batch.judgments {
        j.cluster_rank = None;
    }
    Ok((batch, photo_ids.to_vec()))
}

async fn handle_triage_batch(
    _permit: Option<tokio::sync::OwnedSemaphorePermit>,
    provider: Arc<dyn CuratorProvider>,
    db_path: PathBuf,
    shoot_id: i64,
    photo_ids: Vec<i64>,
    progress: ProgressCounters,
    app: AppHandle,
) {
    let provider_id = provider.provider_id();
    match run_triage_batch(provider.as_ref(), &db_path, shoot_id, &photo_ids).await {
        Ok(batch) => {
            let cents = actual_cost_cents_for_usage(provider_id, &batch.model, &batch.usage);
            progress.cost_cents.fetch_add(cents as usize, Ordering::SeqCst);
            let mut rejects: Vec<i64> = Vec::new();
            if let Ok(db) = Database::open(&crate::db::schema::global_db_path()) {
                let _ = db.add_curator_cost_cents(shoot_id, cents);
                for j in &batch.judgments {
                    // Triage never auto-picks: 'pick' and 'keep' both leave
                    // the photo unreviewed. Only 'reject' is acted on.
                    let flag = if j.suggested_flag
                        == crate::curator::types::SuggestedFlag::Reject
                    {
                        rejects.push(j.photo_id);
                        "reject"
                    } else {
                        "keep"
                    };
                    let _ = db.upsert_triage_judgment(
                        j.photo_id,
                        shoot_id,
                        flag,
                        &j.reason,
                        &batch.model,
                        batch.prompt_version,
                    );
                }
            }
            let _ = app.emit(
                "curator:triage_done",
                json!({ "shootId": shoot_id, "rejectPhotoIds": rejects }),
            );
        }
        Err(e) => {
            // Triage runs unprompted on import; a failure is logged but
            // not surfaced as a curator error toast.
            log::warn!("curator triage batch failed (shoot {}): {:#}", shoot_id, e);
        }
    }
}

async fn run_triage_batch(
    provider: &dyn CuratorProvider,
    db_path: &std::path::Path,
    shoot_id: i64,
    photo_ids: &[i64],
) -> Result<ClusterJudgmentBatch> {
    let db = Database::open(db_path).context("open db in triage task")?;
    let frames = build_stage2_frames(&db, shoot_id, photo_ids)?;
    let frame_refs: Vec<Stage2Frame<'_>> = frames
        .iter()
        .map(|f| Stage2Frame {
            photo_id: f.photo_id,
            thumb_path: &f.thumb_path,
            sharpness_1_10: f.sharpness_1_10,
            face_count: f.face_count,
            eyes_open_count: f.eyes_open_count,
            smile_score: f.smile_score,
        })
        .collect();
    let cluster = Stage2Cluster { frames: &frame_refs };
    provider.run_triage(&cluster).await
}

/// Pre-computed frame info for building Stage 2 calls.
struct Stage2FrameInfo {
    photo_id: i64,
    thumb_path: PathBuf,
    sharpness_1_10: Option<i32>,
    face_count: Option<i32>,
    eyes_open_count: Option<i32>,
    smile_score: Option<f64>,
}

fn build_stage2_frames(
    db: &Database,
    shoot_id: i64,
    photo_ids: &[i64],
) -> Result<Vec<Stage2FrameInfo>> {
    let percentiles = db.sharpness_percentiles_for_shoot(shoot_id).ok();
    let thumb_dir = crate::db::schema::shoot_cache_dir(shoot_id).join("thumbs");

    let mut out = Vec::with_capacity(photo_ids.len());
    for &pid in photo_ids {
        let photo = match db.get_photo_by_id(pid) {
            Ok(p) => p,
            Err(rusqlite::Error::QueryReturnedNoRows) => continue,
            Err(e) => return Err(anyhow::Error::from(e).context("get_photo_by_id")),
        };

        // Map raw sharpness to 1-10 using shoot-wide percentiles. If we
        // have no percentiles yet (e.g. AI worker hasn't run), fall
        // back to None and let the LLM judge without that hint.
        let sharpness_1_10 = match (&percentiles, photo.sharpness_score) {
            (Some(pct), Some(s)) if pct.analyzed_count >= 5 => {
                Some(map_sharpness_to_1_10(s, pct))
            }
            _ => None,
        };

        out.push(Stage2FrameInfo {
            photo_id: pid,
            thumb_path: thumb_dir.join(format!("{}.jpg", pid)),
            sharpness_1_10,
            face_count: photo.face_count,
            eyes_open_count: photo.eyes_open_count,
            smile_score: photo.max_smile_score,
        });
    }
    Ok(out)
}

fn map_sharpness_to_1_10(
    raw: f64,
    p: &crate::db::schema::SharpnessPercentiles,
) -> i32 {
    if raw <= p.p10 {
        1
    } else if raw <= p.p30 {
        3
    } else if raw <= p.p50 {
        5
    } else if raw <= p.p70 {
        7
    } else if raw <= p.p90 {
        9
    } else {
        10
    }
}

/// Pass-through helper (no transformation today). Returns the batch and
/// the photo_ids list as-is. Lives here so we can extend it later (e.g.
/// for is_keeper coercion on `keep`-suggested singletons) without
/// changing the call sites.
fn coerce_singleton_to_pick_keep(
    batch: ClusterJudgmentBatch,
    _is_singleton: bool,
    photo_ids: Vec<i64>,
) -> Result<(ClusterJudgmentBatch, Vec<i64>)> {
    Ok((batch, photo_ids))
}

/// Photo IDs of a cluster's judgments ordered best-first by
/// `cluster_rank` (1 = strongest). Missing ranks sort last; photo_id
/// breaks ties so the ordering is deterministic.
fn rank_judgments(judgments: &[crate::curator::types::CuratorJudgment]) -> Vec<i64> {
    let mut ranked: Vec<(Option<i32>, i64)> = judgments
        .iter()
        .map(|j| (j.cluster_rank, j.photo_id))
        .collect();
    ranked.sort_by(|a, b| {
        let rank_cmp = match (a.0, b.0) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        };
        rank_cmp.then(a.1.cmp(&b.1))
    });
    ranked.into_iter().map(|(_, id)| id).collect()
}

/// Build a single-elimination bracket from a rank-ordered photo list.
/// Each round pairs consecutive survivors; the better-ranked (left)
/// always advances, so the tournament expresses the Curator's ranking as
/// the same pairwise tree the user's own Select tournament produces.
/// Returns `(round, pair, left, right, decision)` tuples.
fn derive_curator_bracket(
    ranked: &[i64],
) -> Vec<(i32, i32, i64, Option<i64>, &'static str)> {
    let mut out = Vec::new();
    let mut survivors: Vec<i64> = ranked.to_vec();
    let mut round = 0i32;
    while survivors.len() > 1 {
        let mut next = Vec::new();
        let mut pair = 0i32;
        let mut i = 0;
        while i < survivors.len() {
            let left = survivors[i];
            if i + 1 < survivors.len() {
                // Left is the better-ranked of the pair, so it wins.
                out.push((round, pair, left, Some(survivors[i + 1]), "L"));
            } else {
                // Odd survivor — a bye straight into the next round.
                out.push((round, pair, left, None, "bye"));
            }
            next.push(left);
            pair += 1;
            i += 2;
        }
        survivors = next;
        round += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_sharpness_buckets_correctly() {
        let p = crate::db::schema::SharpnessPercentiles {
            p10: 10.0,
            p30: 30.0,
            p50: 50.0,
            p70: 70.0,
            p90: 90.0,
            analyzed_count: 100,
            analyzed_max_ts: None,
        };
        assert_eq!(map_sharpness_to_1_10(5.0, &p), 1);
        assert_eq!(map_sharpness_to_1_10(20.0, &p), 3);
        assert_eq!(map_sharpness_to_1_10(40.0, &p), 5);
        assert_eq!(map_sharpness_to_1_10(60.0, &p), 7);
        assert_eq!(map_sharpness_to_1_10(80.0, &p), 9);
        assert_eq!(map_sharpness_to_1_10(95.0, &p), 10);
    }

    #[test]
    fn derive_curator_bracket_even_field() {
        // Four ranked photos → 2 first-round pairs + 1 final = 3 nodes.
        let b = derive_curator_bracket(&[10, 20, 30, 40]);
        assert_eq!(
            b,
            vec![
                (0, 0, 10, Some(20), "L"),
                (0, 1, 30, Some(40), "L"),
                (1, 0, 10, Some(30), "L"),
            ]
        );
    }

    #[test]
    fn derive_curator_bracket_odd_field_gets_a_bye() {
        // Three photos: one first-round pair + a bye, then the final.
        let b = derive_curator_bracket(&[10, 20, 30]);
        assert_eq!(
            b,
            vec![
                (0, 0, 10, Some(20), "L"),
                (0, 1, 30, None, "bye"),
                (1, 0, 10, Some(30), "L"),
            ]
        );
    }

    #[test]
    fn derive_curator_bracket_trivial_fields_are_empty() {
        assert!(derive_curator_bracket(&[]).is_empty());
        assert!(derive_curator_bracket(&[10]).is_empty());
    }
}
