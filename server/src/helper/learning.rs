//! The decision log: what the helper proposed against what the user chose.
//! Recent overrides and accepts become examples in the next run's prompt.

use rusqlite::{params, OptionalExtension};

use crate::{
    db::{helper::now, DbPool},
    outcome::Outcome,
};

/// Why the user overrode a proposal; must match the `decisions` CHECK list.
pub const CRITERIA: &[&str] = &[
    "expression",
    "who",
    "moment",
    "sharpness",
    "duplicate",
    "composition",
    "other",
];

#[derive(Debug, Clone, PartialEq)]
pub struct Example {
    pub proposed: Outcome,
    pub proposed_reason: String,
    pub chosen: Outcome,
    pub criterion: Option<String>,
    pub note: Option<String>,
}

/// Logs the user's outcome against the photo's current proposal, if it has
/// one. `unreviewed` takes the decision back. Changing the choice clears the
/// criterion and note, which explained the earlier choice.
pub fn record_decision(pool: &DbPool, photo_id: i64, chosen: Outcome) -> anyhow::Result<()> {
    let now = now();
    pool.with(|c| {
        let Some((run_id, proposed, reason)) = c
            .query_row(
                "SELECT run_id, outcome, reason FROM proposals WHERE photo_id = ?1",
                [photo_id],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?
        else {
            return Ok(());
        };
        if chosen == Outcome::Unreviewed {
            c.execute(
                "DELETE FROM decisions WHERE photo_id = ?1 AND run_id = ?2",
                params![photo_id, run_id],
            )?;
        } else {
            c.execute(
                "INSERT INTO decisions (photo_id, run_id, proposed, proposed_reason, chosen, decided_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
                 ON CONFLICT(photo_id, run_id) DO UPDATE SET \
                 proposed = excluded.proposed, proposed_reason = excluded.proposed_reason, \
                 criterion = CASE WHEN decisions.chosen = excluded.chosen THEN decisions.criterion END, \
                 note = CASE WHEN decisions.chosen = excluded.chosen THEN decisions.note END, \
                 chosen = excluded.chosen, decided_at = excluded.decided_at",
                params![photo_id, run_id, proposed, reason, chosen.as_str(), now],
            )?;
        }
        Ok(())
    })
}

/// Sets why the user decided as they did, on the photo's latest decision.
/// `Ok(false)` when the photo has no decision.
pub fn set_criterion(
    pool: &DbPool,
    photo_id: i64,
    criterion: &str,
    note: Option<String>,
) -> anyhow::Result<bool> {
    if !CRITERIA.contains(&criterion) {
        anyhow::bail!("unknown criterion {criterion:?}");
    }
    let n = pool.with(|c| {
        c.execute(
            "UPDATE decisions SET criterion = ?2, note = ?3 WHERE rowid = \
             (SELECT rowid FROM decisions WHERE photo_id = ?1 \
              ORDER BY decided_at DESC, rowid DESC LIMIT 1)",
            params![photo_id, criterion, note],
        )
    })?;
    Ok(n > 0)
}

/// Up to `overrides` overrides and `accepts` accepts, newest first.
pub fn recent_examples(
    pool: &DbPool,
    overrides: usize,
    accepts: usize,
) -> anyhow::Result<Vec<Example>> {
    let mut rows: Vec<(String, i64, Example)> = Vec::new();
    for (cmp, limit) in [("!=", overrides), ("=", accepts)] {
        if limit == 0 {
            continue;
        }
        let sql = format!(
            "SELECT decided_at, rowid, proposed, proposed_reason, chosen, criterion, note \
             FROM decisions WHERE chosen {cmp} proposed \
             ORDER BY decided_at DESC, rowid DESC LIMIT ?1"
        );
        let found = pool.with(|c| {
            let mut stmt = c.prepare(&sql)?;
            let it = stmt.query_map([limit as i64], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                ))
            })?;
            it.collect::<Result<Vec<_>, _>>()
        })?;
        for (at, rowid, proposed, proposed_reason, chosen, criterion, note) in found {
            rows.push((
                at,
                rowid,
                Example {
                    proposed: proposed.parse()?,
                    proposed_reason,
                    chosen: chosen.parse()?,
                    criterion,
                    note,
                },
            ));
        }
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    Ok(rows.into_iter().map(|(_, _, e)| e).collect())
}
