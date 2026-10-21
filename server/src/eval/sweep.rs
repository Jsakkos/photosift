//! Burst/moment threshold sweep: how coarse can burst grouping be before two
//! frames the user kept (the exports) fall into one burst? Pure computation
//! over inputs read from the eval scratch database; nothing is written.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Context;
use chrono::NaiveDateTime;

use super::{
    find_export_dirs, match_all_exports, normalize, photo_keys, shoot_by_folder, shoot_hash,
    Skipped,
};
use crate::config::Settings;
use crate::db::{self, DbPool};
use crate::enrich::moments::{split, MomentInput, SplitParams};

pub const PHASH_MAX: &[u32] = &[8, 10, 12, 14, 16, 18, 20, 24];
pub const WINDOW_SECONDS: &[i64] = &[5, 10, 20];
pub const GAP_SECONDS: &[i64] = &[60, 90, 180];

/// How much slack over the current default's collisions a recommendation may take.
const COLLISION_BUDGET: f64 = 0.02;

const DATETIME_FMT: &str = "%Y-%m-%dT%H:%M:%S";

/// One shoot as the sweep sees it: frames in timeline order and the indexes
/// (into `photos`) of the ones the user exported.
#[derive(Debug, Clone)]
pub struct ShootInputs {
    pub photos: Vec<MomentInput>,
    pub truth: HashSet<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SweepRow {
    pub bursts_per_100: f64,
    /// Share of truth photos that share a burst with another truth photo:
    /// bursts too coarse, the user kept two frames of what we called one.
    pub truth_collisions: f64,
    /// Share of truth photos whose burst has at least 2 frames: the burst
    /// grouping is doing work.
    pub near_dupe_coverage: f64,
}

/// Pooled over every shoot (each photo and truth photo counts once).
pub fn evaluate(inputs: &[ShootInputs], params: &SplitParams) -> SweepRow {
    let (mut photos, mut bursts) = (0usize, 0usize);
    let (mut truth, mut collided, mut covered) = (0usize, 0usize, 0usize);
    for shoot in inputs {
        let assigned = split(&shoot.photos, params);
        let mut size: HashMap<usize, usize> = HashMap::new();
        let mut truth_in: HashMap<usize, usize> = HashMap::new();
        for (i, &(_, burst)) in assigned.iter().enumerate() {
            *size.entry(burst).or_default() += 1;
            if shoot.truth.contains(&i) {
                *truth_in.entry(burst).or_default() += 1;
            }
        }
        photos += assigned.len();
        bursts += size.len();
        for (&burst, &n) in &truth_in {
            truth += n;
            if n >= 2 {
                collided += n;
            }
            if size[&burst] >= 2 {
                covered += n;
            }
        }
    }
    let share = |n: usize, of: usize| if of == 0 { 0.0 } else { n as f64 / of as f64 };
    SweepRow {
        bursts_per_100: share(bursts, photos) * 100.0,
        truth_collisions: share(collided, truth),
        near_dupe_coverage: share(covered, truth),
    }
}

/// Every grid point, in grid order (phash, then window, then gap).
pub fn sweep(inputs: &[ShootInputs]) -> Vec<(SplitParams, SweepRow)> {
    let mut rows = Vec::new();
    for &burst_phash_max in PHASH_MAX {
        for &burst_window_seconds in WINDOW_SECONDS {
            for &moment_gap_seconds in GAP_SECONDS {
                let p = SplitParams {
                    moment_gap_seconds,
                    burst_window_seconds,
                    burst_phash_max,
                };
                rows.push((p, evaluate(inputs, &p)));
            }
        }
    }
    rows
}

/// The fewest bursts per 100 photos whose collisions stay within the
/// default's plus 0.02; ties go to fewer collisions, then grid order. Never
/// edits settings.
pub fn recommend<'a>(
    rows: &'a [(SplitParams, SweepRow)],
    default: &SplitParams,
) -> Option<&'a (SplitParams, SweepRow)> {
    let base = rows
        .iter()
        .find(|(p, _)| p == default)
        .map(|(_, r)| r.truth_collisions)?;
    rows.iter()
        .filter(|(_, r)| r.truth_collisions <= base + COLLISION_BUDGET)
        .min_by(|a, b| {
            a.1.bursts_per_100
                .total_cmp(&b.1.bursts_per_100)
                .then(a.1.truth_collisions.total_cmp(&b.1.truth_collisions))
        })
}

pub fn format_row(p: &SplitParams, r: &SweepRow) -> String {
    format!(
        "{:>6} {:>6} {:>5} {:>12.2} {:>11.3} {:>9.3}\n",
        p.burst_phash_max,
        p.burst_window_seconds,
        p.moment_gap_seconds,
        r.bursts_per_100,
        r.truth_collisions,
        r.near_dupe_coverage
    )
}

/// The sweep table, the current default's row and the recommendation.
pub fn format_sweep(rows: &[(SplitParams, SweepRow)], default: &SplitParams) -> String {
    let mut out = format!(
        "{:>6} {:>6} {:>5} {:>12} {:>11} {:>9}\n",
        "phash", "window", "gap", "bursts/100", "collisions", "coverage"
    );
    for (p, r) in rows {
        out.push_str(&format_row(p, r));
    }
    if let Some((p, r)) = rows.iter().find(|(p, _)| p == default) {
        out.push_str("current default:\n");
        out.push_str(&format_row(p, r));
    }
    match recommend(rows, default) {
        Some((p, r)) => {
            out.push_str("recommended (fewest bursts, collisions <= default + 0.02):\n");
            out.push_str(&format_row(p, r));
        }
        None => out.push_str("no recommendation: the default is not on the grid\n"),
    }
    out
}

/// Inputs for each listed shoot already indexed under `eval_dir\root`: frames
/// in timeline order and the originals matched to its `Export\` folders. A
/// shoot that is not indexed or has no export folder is skipped, by hash.
pub fn load_inputs(
    eval_dir: &Path,
    shoots: &[PathBuf],
) -> anyhow::Result<(Vec<ShootInputs>, Vec<Skipped>)> {
    let db_path = eval_dir.join("root").join("photosift.db");
    anyhow::ensure!(
        db_path.is_file(),
        "{} does not exist; run `photosift-eval run` first",
        db_path.display()
    );
    let pool = db::open(&db_path)?;
    let (mut inputs, mut skipped) = (Vec::new(), Vec::new());
    for folder in shoots {
        match shoot_inputs(&pool, folder) {
            Ok(i) => inputs.push(i),
            Err(e) => skipped.push(Skipped {
                shoot_hash: shoot_hash(folder),
                reason: format!("{e:#}"),
            }),
        }
    }
    Ok((inputs, skipped))
}

fn shoot_inputs(pool: &DbPool, folder: &Path) -> anyhow::Result<ShootInputs> {
    let export_dirs = find_export_dirs(folder)?;
    let folder_str = normalize(folder).to_string_lossy().into_owned();
    let shoot_id = shoot_by_folder(pool, &folder_str)?
        .context("not indexed; run `photosift-eval run` first")?;
    type Frame = (i64, Option<String>, Option<i64>);
    let rows: Vec<Frame> = pool.with(|c| {
        let mut stmt = c.prepare(
            "SELECT id, capture_time, phash FROM photos WHERE shoot_id = ?1 \
             AND trashed_at IS NULL AND (raw_path IS NOT NULL OR jpeg_path IS NOT NULL) \
             ORDER BY capture_time, id",
        )?;
        let rows = stmt
            .query_map([shoot_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect();
        rows
    })?;
    let truth_ids: HashSet<i64> = match_all_exports(&export_dirs, &photo_keys(pool, shoot_id)?)
        .matched
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let mut photos = Vec::new();
    let mut truth = HashSet::new();
    for (id, time, phash) in rows {
        let Some(capture_time) =
            time.and_then(|t| NaiveDateTime::parse_from_str(&t, DATETIME_FMT).ok())
        else {
            continue;
        };
        if truth_ids.contains(&id) {
            truth.insert(photos.len());
        }
        photos.push(MomentInput {
            capture_time,
            phash,
        });
    }
    Ok(ShootInputs { photos, truth })
}

/// The grid point the shipped defaults describe.
pub fn current_default() -> SplitParams {
    SplitParams::from(&Settings::default())
}
