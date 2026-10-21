//! Read models for the shoot list and the review screen.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use super::{
    helper::{self, YourCallRow},
    DbPool,
};
use crate::outcome::Outcome;

/// Bursts surfaced in the Highlights strip.
const HIGHLIGHT_COUNT: usize = 8;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub total: i64,
    pub unreviewed: i64,
    pub album: i64,
    pub library: i64,
    pub archive: i64,
    pub delete: i64,
}

impl Counts {
    fn add(&mut self, outcome: &str, n: i64) {
        self.total += n;
        match outcome {
            "unreviewed" => self.unreviewed += n,
            "album" => self.album += n,
            "library" => self.library += n,
            "archive" => self.archive += n,
            "delete" => self.delete += n,
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ShootSummary {
    pub id: i64,
    pub name: Option<String>,
    pub kind: String,
    pub folder: String,
    pub date_start: Option<String>,
    pub date_end: Option<String>,
    pub counts: Counts,
    /// Every live photo has been through the enrichment worker.
    pub enriched: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Burst {
    pub idx: i64,
    /// Best frame first.
    pub photo_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Moment {
    pub idx: i64,
    pub start: Option<String>,
    pub end: Option<String>,
    pub bursts: Vec<Burst>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviewPhoto {
    pub id: i64,
    pub w: Option<i64>,
    pub h: Option<i64>,
    pub capture_time: Option<String>,
    pub camera: Option<String>,
    pub outcome: String,
    pub blooper: bool,
    pub face_count: i64,
    pub signal_score: Option<f64>,
    pub has_video: bool,
    /// The helper's latest proposal; never the outcome itself.
    pub proposal: Option<ProposalView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProposalView {
    pub outcome: Outcome,
    pub blooper: bool,
    pub reason: String,
    /// Why the user overrode it, from this proposal's decision row.
    pub criterion: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HelperView {
    /// `none` (never run), a run status, or `unavailable`.
    pub status: String,
    pub run_id: Option<i64>,
    pub summary: Option<String>,
    pub error: Option<String>,
    /// Proposals by outcome, unreviewed photos only.
    pub suggested: Counts,
    pub your_calls: Vec<YourCallRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShootReview {
    pub shoot: ShootSummary,
    pub counts: Counts,
    pub highlights: Vec<i64>,
    pub moments: Vec<Moment>,
    pub photos: BTreeMap<i64, ReviewPhoto>,
    pub helper: HelperView,
}

/// A live photo plus the grouping columns the worker wrote (not serialized).
struct Row {
    photo: ReviewPhoto,
    moment_idx: Option<i64>,
    burst_idx: Option<i64>,
}

/// Newest shoot first.
pub fn list_shoots(pool: &DbPool) -> anyhow::Result<Vec<ShootSummary>> {
    pool.with(|c| {
        let mut stmt = c.prepare(
            "SELECT id, name, kind, folder, date_start, date_end FROM shoots \
             ORDER BY COALESCE(date_start, substr(created_at, 1, 10)) DESC, id DESC",
        )?;
        let shoots = stmt
            .query_map([], |r| summary_row(c, r))?
            .collect::<Result<Vec<_>, _>>();
        shoots
    })
}

pub fn get_shoot(pool: &DbPool, shoot_id: i64) -> anyhow::Result<Option<ShootSummary>> {
    pool.with(|c| {
        c.query_row(
            "SELECT id, name, kind, folder, date_start, date_end FROM shoots WHERE id = ?1",
            [shoot_id],
            |r| summary_row(c, r),
        )
        .optional()
    })
}

fn summary_row(c: &Connection, r: &rusqlite::Row) -> rusqlite::Result<ShootSummary> {
    let id: i64 = r.get(0)?;
    Ok(ShootSummary {
        id,
        name: r.get(1)?,
        kind: r.get(2)?,
        folder: r.get(3)?,
        date_start: r.get(4)?,
        date_end: r.get(5)?,
        counts: counts_for(c, id)?,
        enriched: c.query_row(
            "SELECT NOT EXISTS (SELECT 1 FROM photos \
             WHERE shoot_id = ?1 AND trashed_at IS NULL AND enriched_at IS NULL)",
            [id],
            |r| r.get(0),
        )?,
    })
}

/// Trashed photos never count.
fn counts_for(c: &Connection, shoot_id: i64) -> rusqlite::Result<Counts> {
    let mut stmt = c.prepare(
        "SELECT outcome, COUNT(*) FROM photos \
         WHERE shoot_id = ?1 AND trashed_at IS NULL GROUP BY outcome",
    )?;
    let mut counts = Counts::default();
    for row in stmt.query_map([shoot_id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
    })? {
        let (outcome, n) = row?;
        counts.add(&outcome, n);
    }
    Ok(counts)
}

pub fn shoot_review(pool: &DbPool, shoot_id: i64) -> anyhow::Result<Option<ShootReview>> {
    let Some(shoot) = get_shoot(pool, shoot_id)? else {
        return Ok(None);
    };
    let rows = pool.with(|c| {
        let mut stmt = c.prepare(
            "SELECT p.id, p.width, p.height, p.capture_time, p.camera, p.outcome, p.blooper, \
                    (SELECT COUNT(*) FROM faces f WHERE f.photo_id = p.id), p.signal_score, \
                    p.video_path IS NOT NULL, p.moment_idx, p.burst_idx \
             FROM photos p WHERE p.shoot_id = ?1 AND p.trashed_at IS NULL \
             ORDER BY p.capture_time IS NULL, p.capture_time, p.id",
        )?;
        let rows = stmt
            .query_map(params![shoot_id], |r| {
                Ok(Row {
                    photo: ReviewPhoto {
                        id: r.get(0)?,
                        w: r.get(1)?,
                        h: r.get(2)?,
                        capture_time: r.get(3)?,
                        camera: r.get(4)?,
                        outcome: r.get(5)?,
                        blooper: r.get::<_, i64>(6)? != 0,
                        face_count: r.get(7)?,
                        signal_score: r.get(8)?,
                        has_video: r.get(9)?,
                        proposal: None,
                    },
                    moment_idx: r.get(10)?,
                    burst_idx: r.get(11)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>();
        rows
    })?;
    let (moments, highlights) = build_timeline(&rows);
    let mut proposals = helper::shoot_proposals(pool, shoot_id)?;
    let photos: BTreeMap<i64, ReviewPhoto> = rows
        .into_iter()
        .map(|mut r| {
            r.photo.proposal = proposals.remove(&r.photo.id).map(|p| ProposalView {
                outcome: p.outcome,
                blooper: p.blooper,
                reason: p.reason,
                criterion: p.criterion,
            });
            (r.photo.id, r.photo)
        })
        .collect();
    let helper = helper_view(pool, shoot_id, &photos)?;
    Ok(Some(ShootReview {
        counts: shoot.counts.clone(),
        shoot,
        highlights,
        moments,
        photos,
        helper,
    }))
}

/// The status is the latest run's; `HelperService` may override it.
fn helper_view(
    pool: &DbPool,
    shoot_id: i64,
    photos: &BTreeMap<i64, ReviewPhoto>,
) -> anyhow::Result<HelperView> {
    let run = helper::latest_run(pool, shoot_id)?;
    let mut suggested = Counts::default();
    for p in photos.values().filter(|p| p.outcome == "unreviewed") {
        if let Some(prop) = &p.proposal {
            suggested.add(prop.outcome.as_str(), 1);
        }
    }
    Ok(HelperView {
        status: run
            .as_ref()
            .map_or_else(|| "none".into(), |r| r.status.clone()),
        run_id: run.as_ref().map(|r| r.id),
        summary: run.as_ref().and_then(|r| r.summary.clone()),
        error: run.and_then(|r| r.error),
        suggested,
        your_calls: helper::your_calls(pool, shoot_id)?,
    })
}

/// `Some` before `None`, higher score first.
fn score_desc(a: Option<f64>, b: Option<f64>) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => b.total_cmp(&a),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// Lays every live photo out as moments > bursts exactly once. Photos the
/// worker has grouped keep its moment/burst indexes. Anything else (not
/// enriched yet, no capture time, or video-only, which grouping skips) becomes
/// a one-photo burst with a fresh burst index: inside the grouped moment whose
/// time span contains it, else in one trailing moment; both in capture-time
/// order. Only grouped bursts are eligible as highlights. `rows` arrive
/// ordered by capture time (undated last), then id.
fn build_timeline(rows: &[Row]) -> (Vec<Moment>, Vec<i64>) {
    struct Slot {
        burst: i64,
        photos: Vec<(i64, Option<f64>)>,
        grouped: bool,
    }
    fn time(r: &Row) -> Option<&str> {
        r.photo.capture_time.as_deref()
    }
    let mut moments: BTreeMap<i64, Vec<Slot>> = BTreeMap::new();
    let mut spans: BTreeMap<i64, (String, String)> = BTreeMap::new();
    let mut next_burst = 0;
    let mut ungrouped = Vec::new();

    for row in rows {
        match (row.moment_idx, row.burst_idx) {
            (Some(m), Some(b)) => {
                next_burst = next_burst.max(b + 1);
                let slots = moments.entry(m).or_default();
                let entry = (row.photo.id, row.photo.signal_score);
                match slots.iter_mut().find(|s| s.burst == b) {
                    Some(slot) => slot.photos.push(entry),
                    None => slots.push(Slot {
                        burst: b,
                        photos: vec![entry],
                        grouped: true,
                    }),
                }
                if let Some(t) = time(row) {
                    let span = spans.entry(m).or_insert((t.to_string(), t.to_string()));
                    span.0 = span.0.as_str().min(t).to_string();
                    span.1 = span.1.as_str().max(t).to_string();
                }
            }
            _ => ungrouped.push(row),
        }
    }

    let tail_idx = moments.keys().next_back().map_or(0, |m| m + 1);
    for row in ungrouped {
        let home = time(row)
            .and_then(|t| {
                spans
                    .iter()
                    .find(|(_, (lo, hi))| lo.as_str() <= t && t <= hi.as_str())
            })
            .map_or(tail_idx, |(m, _)| *m);
        moments.entry(home).or_default().push(Slot {
            burst: next_burst,
            photos: vec![(row.photo.id, row.photo.signal_score)],
            grouped: false,
        });
        next_burst += 1;
    }

    let capture: BTreeMap<i64, &str> = rows
        .iter()
        .filter_map(|r| time(r).map(|t| (r.photo.id, t)))
        .collect();
    let mut highlights: Vec<(f64, i64, i64)> = Vec::new();
    let out = moments
        .into_iter()
        .map(|(idx, mut slots)| {
            slots.sort_by_key(|s| s.burst);
            let times = slots
                .iter()
                .flat_map(|s| s.photos.iter().filter_map(|(id, _)| capture.get(id)));
            let (start, end) = (times.clone().min(), times.max());
            let bursts = slots
                .into_iter()
                .map(|mut s| {
                    s.photos
                        .sort_by(|a, b| score_desc(a.1, b.1).then(a.0.cmp(&b.0)));
                    if let (true, Some((id, Some(score)))) = (s.grouped, s.photos.first()) {
                        highlights.push((*score, s.burst, *id));
                    }
                    Burst {
                        idx: s.burst,
                        photo_ids: s.photos.into_iter().map(|(id, _)| id).collect(),
                    }
                })
                .collect();
            Moment {
                idx,
                start: start.map(|t| t.to_string()),
                end: end.map(|t| t.to_string()),
                bursts,
            }
        })
        .collect();
    highlights.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    let highlights = highlights
        .into_iter()
        .take(HIGHLIGHT_COUNT)
        .map(|(_, _, id)| id)
        .collect();
    (out, highlights)
}
