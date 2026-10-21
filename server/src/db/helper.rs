//! The Claude helper's tables: runs, proposals, your-call pairs and the taste
//! profile. Nothing here touches `photos.outcome` or a sidecar.

use std::collections::HashMap;

use anyhow::{anyhow, bail};
use rusqlite::{params, OptionalExtension, Row};
use serde::Serialize;

use super::DbPool;
use crate::outcome::Outcome;

/// Longest taste profile the helper may store.
pub const TASTE_MAX: usize = 4000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunKind {
    Background,
    Chat,
    External,
    Eval,
}

impl RunKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RunKind::Background => "background",
            RunKind::Chat => "chat",
            RunKind::External => "external",
            RunKind::Eval => "eval",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Queued,
    Running,
    Done,
    Failed,
    Interrupted,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RunStatus::Queued => "queued",
            RunStatus::Running => "running",
            RunStatus::Done => "done",
            RunStatus::Failed => "failed",
            RunStatus::Interrupted => "interrupted",
        }
    }

    fn is_final(self) -> bool {
        matches!(
            self,
            RunStatus::Done | RunStatus::Failed | RunStatus::Interrupted
        )
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RunRow {
    pub id: i64,
    pub shoot_id: i64,
    pub kind: String,
    pub status: String,
    pub session_id: Option<String>,
    pub model: Option<String>,
    pub prompt_version: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub summary: Option<String>,
    pub error: Option<String>,
    pub cost_usd: Option<f64>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
    pub num_turns: Option<i64>,
}

const RUN_COLUMNS: &str = "id, shoot_id, kind, status, session_id, model, prompt_version, \
     started_at, finished_at, summary, error, cost_usd, input_tokens, output_tokens, \
     cache_read_tokens, num_turns";

fn run_row(r: &Row) -> rusqlite::Result<RunRow> {
    Ok(RunRow {
        id: r.get(0)?,
        shoot_id: r.get(1)?,
        kind: r.get(2)?,
        status: r.get(3)?,
        session_id: r.get(4)?,
        model: r.get(5)?,
        prompt_version: r.get(6)?,
        started_at: r.get(7)?,
        finished_at: r.get(8)?,
        summary: r.get(9)?,
        error: r.get(10)?,
        cost_usd: r.get(11)?,
        input_tokens: r.get(12)?,
        output_tokens: r.get(13)?,
        cache_read_tokens: r.get(14)?,
        num_turns: r.get(15)?,
    })
}

/// A helper write to a run that is not `running`: finished, failed,
/// interrupted by a restart, or never started.
#[derive(Debug, Clone, thiserror::Error)]
#[error("run {run_id} is not running (it is {status})")]
pub struct NotRunning {
    pub run_id: i64,
    pub status: String,
}

/// Fails unless the run is `running`. Called on the transaction the caller
/// writes with, so a run the runner has just failed cannot still write.
fn ensure_running(c: &rusqlite::Connection, run_id: i64) -> anyhow::Result<()> {
    let status: Option<String> = c
        .query_row(
            "SELECT status FROM helper_runs WHERE id = ?1",
            [run_id],
            |r| r.get(0),
        )
        .optional()?;
    match status {
        Some(s) if s == RunStatus::Running.as_str() => Ok(()),
        Some(status) => Err(NotRunning { run_id, status }.into()),
        None => bail!("run {run_id} does not exist"),
    }
}

/// What `claude -p` reported for a run. `input_tokens` includes cache writes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunUsage {
    pub cost_usd: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub num_turns: i64,
    pub session_id: String,
}

#[derive(Debug, Clone)]
pub struct NewProposal {
    pub photo_id: i64,
    pub outcome: Outcome,
    pub blooper: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProposalRow {
    pub photo_id: i64,
    pub run_id: i64,
    pub outcome: Outcome,
    pub blooper: bool,
    pub reason: String,
    /// From this proposal's decision row, when the user has given one.
    pub criterion: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct YourCallRow {
    pub id: i64,
    pub a: i64,
    pub b: i64,
    pub tradeoff: String,
}

pub(crate) fn now() -> String {
    chrono::Local::now()
        .naive_local()
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string()
}

fn outcome_col(r: &Row, idx: usize) -> rusqlite::Result<Outcome> {
    let s: String = r.get(idx)?;
    s.parse().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e))
    })
}

/// A new run in `queued`.
pub fn create_run(
    pool: &DbPool,
    shoot_id: i64,
    kind: RunKind,
    model: Option<&str>,
    prompt_version: Option<&str>,
) -> anyhow::Result<i64> {
    pool.with(|c| {
        c.execute(
            "INSERT INTO helper_runs (shoot_id, kind, status, model, prompt_version) \
             VALUES (?1, ?2, 'queued', ?3, ?4)",
            params![shoot_id, kind.as_str(), model, prompt_version],
        )?;
        Ok(c.last_insert_rowid())
    })
}

/// Moves a run to `status`. Entering `running` stamps `started_at`; a final
/// status stamps `finished_at`. `error` replaces the stored error.
pub fn set_run_status(
    pool: &DbPool,
    run_id: i64,
    status: RunStatus,
    error: Option<&str>,
) -> anyhow::Result<()> {
    let now = now();
    let n = pool.with(|c| {
        c.execute(
            "UPDATE helper_runs SET status = ?2, error = ?3, \
             started_at = CASE WHEN ?2 = 'running' THEN COALESCE(started_at, ?4) ELSE started_at END, \
             finished_at = CASE WHEN ?5 THEN ?4 ELSE finished_at END \
             WHERE id = ?1",
            params![run_id, status.as_str(), error, now, status.is_final()],
        )
    })?;
    if n == 0 {
        bail!("run {run_id} does not exist");
    }
    Ok(())
}

/// Records what `claude -p` reported. The status is the caller's business.
pub fn finish_run(pool: &DbPool, run_id: i64, usage: &RunUsage) -> anyhow::Result<()> {
    pool.with(|c| {
        c.execute(
            "UPDATE helper_runs SET cost_usd = ?2, input_tokens = ?3, output_tokens = ?4, \
             cache_read_tokens = ?5, num_turns = ?6, session_id = ?7 WHERE id = ?1",
            params![
                run_id,
                usage.cost_usd,
                usage.input_tokens,
                usage.output_tokens,
                usage.cache_read_tokens,
                usage.num_turns,
                usage.session_id,
            ],
        )
    })?;
    Ok(())
}

/// The helper's `finish_run` tool: the summary, and status `done`. Only a
/// `running` run can finish; check and write are one statement, so a run the
/// runner has just failed stays failed.
pub fn complete_run(pool: &DbPool, run_id: i64, summary: &str) -> anyhow::Result<()> {
    let now = now();
    let n = pool.with(|c| {
        c.execute(
            "UPDATE helper_runs SET summary = ?2, status = 'done', error = NULL, finished_at = ?3 \
             WHERE id = ?1 AND status = 'running'",
            params![run_id, summary, now],
        )
    })?;
    if n == 0 {
        let status = get_run(pool, run_id)?
            .ok_or_else(|| anyhow!("run {run_id} does not exist"))?
            .status;
        return Err(NotRunning { run_id, status }.into());
    }
    Ok(())
}

/// `queued` -> `running`. False when the run is no longer queued.
pub fn start_queued_run(pool: &DbPool, run_id: i64) -> anyhow::Result<bool> {
    let now = now();
    let n = pool.with(|c| {
        c.execute(
            "UPDATE helper_runs SET status = 'running', started_at = ?2 \
             WHERE id = ?1 AND status = 'queued'",
            params![run_id, now],
        )
    })?;
    Ok(n > 0)
}

/// Moves a `queued` or `running` run to the final `status`. False when it had
/// already ended (e.g. `finish_run` made it `done`); that run is left alone.
pub fn end_open_run(
    pool: &DbPool,
    run_id: i64,
    status: RunStatus,
    error: Option<&str>,
) -> anyhow::Result<bool> {
    if !status.is_final() {
        bail!("{} is not a final status", status.as_str());
    }
    let now = now();
    let n = pool.with(|c| {
        c.execute(
            "UPDATE helper_runs SET status = ?2, error = ?3, finished_at = ?4 \
             WHERE id = ?1 AND status IN ('queued', 'running')",
            params![run_id, status.as_str(), error, now],
        )
    })?;
    Ok(n > 0)
}

/// Marks every `queued` or `running` run `interrupted` and returns them as
/// they were. Run at startup: no `claude` from before a restart may write.
pub fn interrupt_open_runs(pool: &DbPool) -> anyhow::Result<Vec<RunRow>> {
    let now = now();
    pool.with(|c| {
        let tx = c.unchecked_transaction()?;
        let rows = {
            let mut stmt = tx.prepare(&format!(
                "SELECT {RUN_COLUMNS} FROM helper_runs WHERE status IN ('queued', 'running') \
                 ORDER BY id"
            ))?;
            let rows = stmt
                .query_map([], run_row)?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        tx.execute(
            "UPDATE helper_runs SET status = 'interrupted', \
             error = 'PhotoSift restarted before the run finished', finished_at = ?1 \
             WHERE status IN ('queued', 'running')",
            [&now],
        )?;
        tx.commit()?;
        Ok(rows)
    })
}

/// The shoot's newest `queued` or `running` run.
pub fn open_run(pool: &DbPool, shoot_id: i64) -> anyhow::Result<Option<RunRow>> {
    pool.with(|c| {
        c.query_row(
            &format!(
                "SELECT {RUN_COLUMNS} FROM helper_runs \
                 WHERE shoot_id = ?1 AND status IN ('queued', 'running') ORDER BY id DESC LIMIT 1"
            ),
            [shoot_id],
            run_row,
        )
        .optional()
    })
}

/// Live, unreviewed photos without a current proposal: none at all, or one
/// from a run that is not `done` or finished before the photo was enriched.
const UNPROPOSED: &str = "SELECT p.id FROM photos p \
     WHERE p.shoot_id = ?1 AND p.trashed_at IS NULL AND p.outcome = 'unreviewed' \
     AND NOT EXISTS (SELECT 1 FROM proposals pr JOIN helper_runs r ON r.id = pr.run_id \
        WHERE pr.photo_id = p.id AND r.status = 'done' AND r.finished_at >= p.enriched_at) \
     ORDER BY p.id";

fn unproposed(pool: &DbPool, shoot_id: i64) -> anyhow::Result<Vec<i64>> {
    pool.with(|c| {
        let mut stmt = c.prepare(UNPROPOSED)?;
        let ids = stmt.query_map([shoot_id], |r| r.get(0))?.collect();
        ids
    })
}

/// False when every live, unreviewed photo already has a proposal from a
/// `done` run newer than its enrichment, so an automatic run has nothing to do.
pub fn shoot_needs_run(pool: &DbPool, shoot_id: i64) -> anyhow::Result<bool> {
    Ok(!unproposed(pool, shoot_id)?.is_empty())
}

/// The photos a follow-up run should focus on: empty while no photo of the
/// shoot has a proposal (the run covers the whole shoot), else the live,
/// unreviewed photos still without a current one.
pub fn new_photo_ids(pool: &DbPool, shoot_id: i64) -> anyhow::Result<Vec<i64>> {
    let any: bool = pool.with(|c| {
        c.query_row(
            "SELECT EXISTS (SELECT 1 FROM proposals pr JOIN photos p ON p.id = pr.photo_id \
             WHERE p.shoot_id = ?1)",
            [shoot_id],
            |r| r.get(0),
        )
    })?;
    if !any {
        return Ok(Vec::new());
    }
    unproposed(pool, shoot_id)
}

pub fn get_run(pool: &DbPool, run_id: i64) -> anyhow::Result<Option<RunRow>> {
    pool.with(|c| {
        c.query_row(
            &format!("SELECT {RUN_COLUMNS} FROM helper_runs WHERE id = ?1"),
            [run_id],
            run_row,
        )
        .optional()
    })
}

/// The run, if it is `running`. Every helper write checks this first, so an
/// orphaned `claude` from before a restart (its run now `interrupted`) or a
/// finished run cannot write.
pub fn active_run(pool: &DbPool, run_id: i64) -> anyhow::Result<RunRow> {
    let run = get_run(pool, run_id)?.ok_or_else(|| anyhow!("run {run_id} does not exist"))?;
    if run.status != RunStatus::Running.as_str() {
        bail!("run {run_id} is not running (it is {})", run.status);
    }
    Ok(run)
}

pub fn latest_run(pool: &DbPool, shoot_id: i64) -> anyhow::Result<Option<RunRow>> {
    pool.with(|c| {
        c.query_row(
            &format!(
                "SELECT {RUN_COLUMNS} FROM helper_runs WHERE shoot_id = ?1 ORDER BY id DESC LIMIT 1"
            ),
            [shoot_id],
            run_row,
        )
        .optional()
    })
}

/// Writes all proposals or none, and only while the run is `running`
/// (`NotRunning` otherwise). Validation is the caller's; the schema still
/// refuses an `unreviewed` outcome.
pub fn upsert_proposals(pool: &DbPool, run_id: i64, items: &[NewProposal]) -> anyhow::Result<()> {
    let now = now();
    pool.with_tx(|tx| {
        ensure_running(tx, run_id)?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO proposals (photo_id, run_id, outcome, blooper, reason, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
                 ON CONFLICT(photo_id) DO UPDATE SET run_id = excluded.run_id, \
                 outcome = excluded.outcome, blooper = excluded.blooper, \
                 reason = excluded.reason, created_at = excluded.created_at",
            )?;
            for p in items {
                stmt.execute(params![
                    p.photo_id,
                    run_id,
                    p.outcome.as_str(),
                    p.blooper,
                    p.reason,
                    now
                ])?;
            }
        }
        Ok(())
    })
}

#[derive(Debug, thiserror::Error)]
pub enum YourCallError {
    #[error("photos {0} and {1} already form a your-call pair in this run")]
    Duplicate(i64, i64),
    #[error(transparent)]
    NotRunning(NotRunning),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Stores the pair with the lower id first, so `(b, a)` collides with `(a, b)`.
/// A pair from an earlier run is taken over by this one; the same run naming
/// it twice is refused. Only a `running` run may add one.
pub fn add_your_call(
    pool: &DbPool,
    run_id: i64,
    a: i64,
    b: i64,
    tradeoff: &str,
) -> Result<i64, YourCallError> {
    let (lo, hi) = (a.min(b), a.max(b));
    let stored = pool.with_tx(|tx| {
        ensure_running(tx, run_id)?;
        let shoot: i64 = tx.query_row(
            "SELECT shoot_id FROM helper_runs WHERE id = ?1",
            [run_id],
            |r| r.get(0),
        )?;
        let existing: Option<(i64, i64)> = tx
            .query_row(
                "SELECT id, run_id FROM your_call_pairs WHERE photo_a = ?1 AND photo_b = ?2",
                [lo, hi],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let id = match existing {
            Some((_, run)) if run == run_id => return Ok(None),
            Some((id, _)) => {
                tx.execute(
                    "UPDATE your_call_pairs SET run_id = ?2, shoot_id = ?3, tradeoff = ?4                      WHERE id = ?1",
                    params![id, run_id, shoot, tradeoff],
                )?;
                id
            }
            None => {
                tx.execute(
                    "INSERT INTO your_call_pairs (run_id, shoot_id, photo_a, photo_b, tradeoff)                      VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![run_id, shoot, lo, hi, tradeoff],
                )?;
                tx.last_insert_rowid()
            }
        };
        Ok(Some(id))
    });
    match stored {
        Ok(Some(id)) => Ok(id),
        Ok(None) => Err(YourCallError::Duplicate(lo, hi)),
        Err(e) => Err(match e.downcast::<NotRunning>() {
            Ok(nr) => YourCallError::NotRunning(nr),
            Err(e) => YourCallError::Other(e),
        }),
    }
}

/// Empty until one has been written.
pub fn get_taste(pool: &DbPool) -> anyhow::Result<String> {
    Ok(pool
        .with(|c| {
            c.query_row("SELECT text FROM taste WHERE id = 1", [], |r| r.get(0))
                .optional()
        })?
        .unwrap_or_default())
}

pub fn set_taste(pool: &DbPool, text: &str) -> anyhow::Result<()> {
    let len = text.chars().count();
    if len > TASTE_MAX {
        bail!("the taste profile is {len} characters; the limit is {TASTE_MAX}");
    }
    let now = now();
    pool.with(|c| {
        c.execute(
            "INSERT INTO taste (id, text, updated_at) VALUES (1, ?1, ?2) \
             ON CONFLICT(id) DO UPDATE SET text = excluded.text, updated_at = excluded.updated_at",
            params![text, now],
        )
    })?;
    Ok(())
}

/// Proposals for the shoot's photos (trashed ones included), keyed by photo.
pub fn shoot_proposals(pool: &DbPool, shoot_id: i64) -> anyhow::Result<HashMap<i64, ProposalRow>> {
    pool.with(|c| {
        let mut stmt = c.prepare(
            "SELECT pr.photo_id, pr.run_id, pr.outcome, pr.blooper, pr.reason, d.criterion \
             FROM proposals pr JOIN photos p ON p.id = pr.photo_id \
             LEFT JOIN decisions d ON d.photo_id = pr.photo_id AND d.run_id = pr.run_id \
             WHERE p.shoot_id = ?1",
        )?;
        let rows = stmt.query_map([shoot_id], |r| {
            Ok(ProposalRow {
                photo_id: r.get(0)?,
                run_id: r.get(1)?,
                outcome: outcome_col(r, 2)?,
                blooper: r.get::<_, i64>(3)? != 0,
                reason: r.get(4)?,
                criterion: r.get(5)?,
            })
        })?;
        rows.map(|r| r.map(|p| (p.photo_id, p))).collect()
    })
}

/// The shoot's pairs whose photos are both still live, oldest first.
pub fn your_calls(pool: &DbPool, shoot_id: i64) -> anyhow::Result<Vec<YourCallRow>> {
    pool.with(|c| {
        let mut stmt = c.prepare(
            "SELECT y.id, y.photo_a, y.photo_b, y.tradeoff FROM your_call_pairs y \
             JOIN photos a ON a.id = y.photo_a AND a.trashed_at IS NULL \
             JOIN photos b ON b.id = y.photo_b AND b.trashed_at IS NULL \
             WHERE y.shoot_id = ?1 ORDER BY y.id",
        )?;
        let rows = stmt.query_map([shoot_id], |r| {
            Ok(YourCallRow {
                id: r.get(0)?,
                a: r.get(1)?,
                b: r.get(2)?,
                tradeoff: r.get(3)?,
            })
        })?;
        rows.collect()
    })
}
