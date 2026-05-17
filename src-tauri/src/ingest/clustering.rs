use super::phash::hamming_distance;

/// Default perceptual-hash hamming distance at or under which two photos
/// are treated as part of the same group. Deliberately loose: grouping
/// only needs to funnel similar frames into the Select tournament, not be
/// perfect — the inline regroup control lets the user retune per shoot.
pub const DEFAULT_GROUP_THRESHOLD: u32 = 16;
/// Default capture-time gap (seconds) allowed between two pHash-similar
/// photos before they stop being considered part of the same burst.
/// 60s comfortably covers typical D750 burst sequences plus a beat of
/// recomposition; past this, the similarity is more likely a pHash
/// false-positive across distinct moments.
pub const DEFAULT_TIME_WINDOW_S: u32 = 60;

struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]);
        }
        self.parent[x]
    }

    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra == rb {
            return;
        }
        if self.rank[ra] < self.rank[rb] {
            self.parent[ra] = rb;
        } else if self.rank[ra] > self.rank[rb] {
            self.parent[rb] = ra;
        } else {
            self.parent[rb] = ra;
            self.rank[ra] += 1;
        }
    }

    fn components(&mut self) -> Vec<Vec<usize>> {
        let n = self.parent.len();
        let mut groups: std::collections::HashMap<usize, Vec<usize>> =
            std::collections::HashMap::new();
        for i in 0..n {
            let root = self.find(i);
            groups.entry(root).or_default().push(i);
        }
        groups.into_values().filter(|g| g.len() >= 2).collect()
    }
}

pub struct GroupResult {
    pub member_indices: Vec<usize>,
}

/// Input row for clustering: photo id, perceptual hash, and an
/// optional capture time (unix seconds). Photos missing a capture
/// time fall back to pHash-only similarity (the time-window check
/// passes for them).
pub type PhashRow = (i64, [u8; 8], Option<i64>);

/// Disjoint clustering by perceptual-hash distance, with an optional
/// capture-time window to reject pHash collisions across distinct
/// moments. Every photo belongs to at most one emitted group.
///
/// - A pair forms an edge if `hamming(a, b) <= threshold` AND the
///   capture-time gap is ≤ `time_window_s` (or either side's time is
///   unknown). `time_window_s == 0` disables the time constraint.
/// - Groups are the transitive closure of those edges; only components
///   with at least 2 members are emitted.
pub fn cluster_phashes(
    rows: &[PhashRow],
    threshold: u32,
    time_window_s: u32,
) -> Vec<GroupResult> {
    let n = rows.len();
    if n < 2 {
        return Vec::new();
    }

    let mut uf = UnionFind::new(n);

    let time_ok = |i: usize, j: usize| -> bool {
        if time_window_s == 0 {
            return true;
        }
        match (rows[i].2, rows[j].2) {
            (Some(ti), Some(tj)) => (ti - tj).unsigned_abs() as u32 <= time_window_s,
            // If either side lacks a timestamp, fall back to hash-only.
            _ => true,
        }
    };

    for i in 0..n {
        for j in (i + 1)..n {
            if !time_ok(i, j) {
                continue;
            }
            if hamming_distance(&rows[i].1, &rows[j].1) <= threshold {
                uf.union(i, j);
            }
        }
    }

    uf.components()
        .into_iter()
        .map(|member_indices| GroupResult { member_indices })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_cluster(phashes: &[(i64, [u8; 8])]) -> Vec<GroupResult> {
        // Map the test-friendly 2-tuple to the 3-tuple the production
        // API takes, with no timestamps. Time window = 0 → disabled.
        let rows: Vec<PhashRow> = phashes.iter().map(|(id, h)| (*id, *h, None)).collect();
        cluster_phashes(&rows, DEFAULT_GROUP_THRESHOLD, 0)
    }

    #[test]
    fn test_basic_cluster() {
        let phashes = vec![
            (1, [0x00; 8]),
            (2, [0x00; 8]),
            (3, [0xFF; 8]),
        ];
        let groups = default_cluster(&phashes);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].member_indices.len(), 2);
    }

    #[test]
    fn test_overlap_merges_into_single_group() {
        // Three photos: h0 and h1 are identical, h2 is 8 bits away —
        // within the default threshold. They form one connected
        // component, so all three land in a single group and no photo
        // lives in two groups at once.
        let h0 = [0x00u8; 8];
        let h1 = [0x00u8; 8];
        let mut h2 = [0x00u8; 8];
        h2[0] = 0xFF;

        let phashes = vec![(1, h0), (2, h1), (3, h2)];
        let groups = default_cluster(&phashes);

        assert_eq!(groups.len(), 1, "expected exactly one merged group");
        assert_eq!(groups[0].member_indices.len(), 3);
    }

    #[test]
    fn test_every_photo_belongs_to_at_most_one_group() {
        // Regression guard for the user-reported "a photo appears in
        // two groups" bug. Constructs a cluster with members at varying
        // distances, then checks the invariant across the full output.
        let h0 = [0x00u8; 8];
        let h1 = [0x00u8; 8];
        let mut h2 = [0x00u8; 8];
        h2[0] = 0x0F; // 4 bits
        let mut h3 = [0x00u8; 8];
        h3[0] = 0xFF; // 8 bits

        let phashes = vec![(1, h0), (2, h1), (3, h2), (4, h3)];
        let groups = default_cluster(&phashes);

        let mut seen = std::collections::HashSet::new();
        for g in &groups {
            for &idx in &g.member_indices {
                assert!(
                    seen.insert(idx),
                    "photo index {idx} appeared in more than one group",
                );
            }
        }
    }

    #[test]
    fn test_no_groups_for_distant_hashes() {
        let phashes = vec![
            (1, [0x00; 8]),
            (2, [0xFF; 8]),
        ];
        let groups = default_cluster(&phashes);
        assert!(groups.is_empty());
    }

    #[test]
    fn test_tighter_threshold_splits_group() {
        // Two hashes at hamming distance 4 group together at threshold 4
        // but split apart at threshold 2.
        let h0 = [0x00u8; 8];
        let mut h1 = [0x00u8; 8];
        h1[0] = 0x0F; // 4 bits different

        let rows = vec![(1, h0, None), (2, h1, None)];

        assert_eq!(cluster_phashes(&rows, 4, 0).len(), 1);
        assert!(cluster_phashes(&rows, 2, 0).is_empty());
    }

    #[test]
    fn test_time_window_rejects_cross_time_phash_collision() {
        // Two photos with identical pHashes but captured far apart in
        // time. Without the window they form a group; with a 60s window
        // they don't.
        let h = [0x00u8; 8];
        let near: Vec<PhashRow> = vec![(1, h, Some(1000)), (2, h, Some(1030))];
        let far: Vec<PhashRow> = vec![(1, h, Some(1000)), (2, h, Some(9999))];

        assert_eq!(cluster_phashes(&near, 12, 60).len(), 1, "30s gap within window");
        assert!(cluster_phashes(&far, 12, 60).is_empty(), "9000s gap filtered");

        // time_window_s=0 disables the filter.
        assert_eq!(cluster_phashes(&far, 12, 0).len(), 1, "zero window disables check");
    }

    #[test]
    fn test_time_window_falls_back_to_phash_only_when_time_missing() {
        // If either photo lacks a capture time, the pair is treated as
        // time-compatible so pHash similarity still drives clustering.
        let h = [0x00u8; 8];
        let rows: Vec<PhashRow> = vec![(1, h, Some(1000)), (2, h, None)];
        assert_eq!(cluster_phashes(&rows, 12, 60).len(), 1);
    }
}
