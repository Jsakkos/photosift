//! Scoring a set of picks against what the user actually exported.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Score {
    /// Picks that are truth photos.
    pub exact: usize,
    /// Picks paired one-to-one with a remaining truth photo of the same burst.
    pub near: usize,
    /// Truth photos neither picked nor covered by a near pick.
    pub missed: usize,
    /// Picks that are neither exact nor near.
    pub false_picks: usize,
    /// exact / |keep|.
    pub precision_strict: f64,
    /// exact / |truth|.
    pub recall_strict: f64,
    /// (exact + near) / |truth|.
    pub recall_lenient: f64,
}

/// `burst_of` maps a photo to its (moment, burst). A photo missing from it
/// shares a burst with nothing. Empty denominators give 0.
pub fn score(
    keep: &HashSet<i64>,
    truth: &HashSet<i64>,
    burst_of: &HashMap<i64, (i64, i64)>,
) -> Score {
    let exact = keep.intersection(truth).count();
    let mut picks: HashMap<(i64, i64), usize> = HashMap::new();
    for id in keep.difference(truth) {
        if let Some(b) = burst_of.get(id) {
            *picks.entry(*b).or_default() += 1;
        }
    }
    let mut truths: HashMap<(i64, i64), usize> = HashMap::new();
    for id in truth.difference(keep) {
        if let Some(b) = burst_of.get(id) {
            *truths.entry(*b).or_default() += 1;
        }
    }
    // Within one burst every remaining pick can stand in for any remaining
    // truth photo, so the largest one-to-one matching is the smaller count.
    let near: usize = picks
        .iter()
        .map(|(b, &n)| n.min(truths.get(b).copied().unwrap_or(0)))
        .sum();
    let ratio = |n: usize, d: usize| if d == 0 { 0.0 } else { n as f64 / d as f64 };
    Score {
        exact,
        near,
        missed: truth.len() - exact - near,
        false_picks: keep.len() - exact - near,
        precision_strict: ratio(exact, keep.len()),
        recall_strict: ratio(exact, truth.len()),
        recall_lenient: ratio(exact + near, truth.len()),
    }
}
