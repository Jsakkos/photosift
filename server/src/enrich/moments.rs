//! Groups a shoot's photos into moments (review-screen sections) and bursts
//! (the unit a pick is made from), and ranks frames with a simple signal score.

use chrono::NaiveDateTime;
use rusqlite::params;

use crate::config::Settings;
use crate::db::DbPool;
use crate::media::phash::hamming;

const DATETIME_FMT: &str = "%Y-%m-%dT%H:%M:%S";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MomentInput {
    pub capture_time: NaiveDateTime,
    /// None when the preview could not be rendered; such a frame never joins a
    /// burst by similarity.
    pub phash: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitParams {
    pub moment_gap_seconds: i64,
    pub burst_window_seconds: i64,
    pub burst_phash_max: u32,
}

impl From<&Settings> for SplitParams {
    fn from(s: &Settings) -> Self {
        Self {
            moment_gap_seconds: s.moment_gap_seconds,
            burst_window_seconds: s.burst_window_seconds,
            burst_phash_max: s.burst_phash_max,
        }
    }
}

/// Assigns `(moment_idx, burst_idx)` to time-sorted frames.
///
/// A new moment starts when the gap to the previous frame exceeds
/// `moment_gap_seconds`. A new burst starts at a new moment, when the gap to the
/// previous frame exceeds `burst_window_seconds`, or when the pHash distance to
/// the burst's FIRST frame exceeds `burst_phash_max` (comparing to the first
/// frame stops a slow drift from chaining unrelated poses together). A frame
/// without a pHash, or following one, always starts its own burst. Burst
/// indexes run across moments.
pub fn split(photos: &[MomentInput], params: &SplitParams) -> Vec<(usize, usize)> {
    let mut out = Vec::with_capacity(photos.len());
    let (mut moment, mut burst) = (0usize, 0usize);
    // Hash of the current burst's first frame; None when that frame had no hash.
    let mut burst_first: Option<i64> = None;
    for (i, p) in photos.iter().enumerate() {
        if i == 0 {
            burst_first = p.phash;
        } else {
            let gap = (p.capture_time - photos[i - 1].capture_time).num_seconds();
            let new_moment = gap > params.moment_gap_seconds;
            let same_pose = match (burst_first, p.phash) {
                (Some(first), Some(h)) => hamming(first, h) <= params.burst_phash_max,
                _ => false,
            };
            if new_moment {
                moment += 1;
            }
            if new_moment || gap > params.burst_window_seconds || !same_pose {
                burst += 1;
                burst_first = p.phash;
            }
        }
        out.push((moment, burst));
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Signals {
    pub face_count: usize,
    /// Area of the largest face box as a fraction of the frame (0-1).
    pub largest_face_area: f32,
    pub face_sharpness: Option<f64>,
    pub sharpness: f64,
    pub clip_hi: f64,
    pub clip_lo: f64,
}

// Positive terms sum to 1.0; clipping subtracts. Every term is normalized to 0-1 first.
// A face is the subject of almost every family photo, so having one matters most.
const W_FACE: f32 = 0.25;
// A larger face is a more deliberate framing of the subject.
const W_FACE_SIZE: f32 = 0.10;
// Focus on the face decides a keeper far more often than focus elsewhere.
const W_FACE_SHARP: f32 = 0.30;
// Whole-frame sharpness catches camera shake and misfocus, and ranks faceless frames.
const W_SHARP: f32 = 0.35;
// Blown highlights are unrecoverable, so they cost the most.
const W_CLIP_HI: f32 = 1.5;
// Crushed shadows are often intentional or recoverable from RAW, so they cost less.
const W_CLIP_LO: f32 = 0.5;
/// Laplacian variance (at the 1024 px analysis size) treated as fully sharp.
const SHARP_REF: f64 = 1000.0;
/// Face area (fraction of frame) treated as a full-size subject.
const FACE_AREA_REF: f32 = 0.05;

/// Log-scaled so doubling sharpness always helps but never dominates; saturates at `SHARP_REF`.
fn normalized_sharpness(v: f64) -> f32 {
    if !v.is_finite() || v <= 0.0 {
        return 0.0;
    }
    ((v.ln_1p() / SHARP_REF.ln_1p()) as f32).min(1.0)
}

fn fraction(v: f64) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0) as f32
    } else {
        0.0
    }
}

/// Temporary, explainable ranking for Highlights and in-burst ordering.
/// Higher is better; the range is roughly -1 to 1 and only the order matters.
pub fn signal_score(s: &Signals) -> f32 {
    let mut score = W_SHARP * normalized_sharpness(s.sharpness);
    if s.face_count > 0 {
        score += W_FACE;
        score += W_FACE_SIZE * (s.largest_face_area / FACE_AREA_REF).clamp(0.0, 1.0);
        score += W_FACE_SHARP * s.face_sharpness.map_or(0.0, normalized_sharpness);
    }
    score - W_CLIP_HI * fraction(s.clip_hi) - W_CLIP_LO * fraction(s.clip_lo)
}

struct Candidate {
    id: i64,
    time: Option<NaiveDateTime>,
    phash: Option<i64>,
    signals: Signals,
}

/// Writes `moment_idx`, `burst_idx` and `signal_score` for a shoot's live
/// photos (not trashed, not video-only), ordered by capture time then id.
/// Excluded rows are reset to NULL so indexes stay contiguous after a trash.
/// A photo with no capture time gets a score but no moment or burst.
pub fn assign(db: &DbPool, shoot_id: i64, settings: &Settings) -> anyhow::Result<()> {
    // Read, group and write under one lock and one transaction: the trash
    // handler and the enrichment worker both call this, and a trash landing
    // between a separate read and write would be overwritten with stale rows.
    db.with(|c| {
        let tx = c.unchecked_transaction()?;
        let candidates = read_candidates(&tx, shoot_id)?;

        let timed: Vec<(&Candidate, NaiveDateTime)> = candidates
            .iter()
            .filter_map(|c| c.time.map(|t| (c, t)))
            .collect();
        let inputs: Vec<MomentInput> = timed
            .iter()
            .map(|(c, t)| MomentInput {
                capture_time: *t,
                phash: c.phash,
            })
            .collect();
        let groups = split(&inputs, &SplitParams::from(settings));

        tx.execute(
            "UPDATE photos SET moment_idx = NULL, burst_idx = NULL, signal_score = NULL              WHERE shoot_id = ?1 AND (trashed_at IS NOT NULL                 OR (raw_path IS NULL AND jpeg_path IS NULL))",
            [shoot_id],
        )?;
        {
            let mut set = tx.prepare(
                "UPDATE photos SET moment_idx = ?1, burst_idx = ?2, signal_score = ?3                  WHERE id = ?4",
            )?;
            for ((cand, _), (moment, burst)) in timed.iter().zip(&groups) {
                let score = f64::from(signal_score(&cand.signals));
                set.execute(params![*moment as i64, *burst as i64, score, cand.id])?;
            }
            for cand in candidates.iter().filter(|c| c.time.is_none()) {
                let score = f64::from(signal_score(&cand.signals));
                set.execute(params![None::<i64>, None::<i64>, score, cand.id])?;
            }
        }
        tx.commit()
    })
}

fn read_candidates(c: &rusqlite::Connection, shoot_id: i64) -> rusqlite::Result<Vec<Candidate>> {
    let mut stmt = c.prepare(
        "SELECT p.id, p.capture_time, p.phash, p.sharpness, p.face_sharpness,                 p.clip_hi, p.clip_lo, COUNT(f.id), COALESCE(MAX(f.w * f.h), 0.0)          FROM photos p LEFT JOIN faces f ON f.photo_id = p.id          WHERE p.shoot_id = ?1 AND p.trashed_at IS NULL            AND (p.raw_path IS NOT NULL OR p.jpeg_path IS NOT NULL)          GROUP BY p.id          ORDER BY p.capture_time, p.id",
    )?;
    let rows = stmt
        .query_map([shoot_id], |r| {
            let time: Option<String> = r.get(1)?;
            Ok(Candidate {
                id: r.get(0)?,
                time: time.and_then(|t| NaiveDateTime::parse_from_str(&t, DATETIME_FMT).ok()),
                phash: r.get(2)?,
                signals: Signals {
                    face_count: r.get::<_, i64>(7)? as usize,
                    largest_face_area: r.get::<_, f64>(8)? as f32,
                    face_sharpness: r.get(4)?,
                    sharpness: r.get::<_, Option<f64>>(3)?.unwrap_or(0.0),
                    clip_hi: r.get::<_, Option<f64>>(5)?.unwrap_or(0.0),
                    clip_lo: r.get::<_, Option<f64>>(6)?.unwrap_or(0.0),
                },
            })
        })?
        .collect::<Result<Vec<_>, _>>();
    rows
}
