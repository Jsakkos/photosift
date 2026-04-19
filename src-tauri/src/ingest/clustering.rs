use super::phash::hamming_distance;

pub const DEFAULT_NEAR_DUP_THRESHOLD: u32 = 4;
pub const DEFAULT_RELATED_THRESHOLD: u32 = 12;

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
    pub group_type: &'static str,
    pub member_indices: Vec<usize>,
}

/// Disjoint clustering by perceptual-hash distance. Every photo belongs
/// to at most one emitted group.
///
/// - A group forms whenever two photos are within `related_threshold`
///   hamming distance (transitive closure).
/// - Each emitted group is labelled `"near_duplicate"` when *all* of
///   its members share the same near-duplicate root (i.e. no
///   intra-group hop exceeds `near_dup_threshold`). Otherwise it's
///   labelled `"related"`. This collapses the old two-tier scheme —
///   which emitted the tight near-dup group *and* a looser related
///   group covering the same photos — into a single set.
pub fn cluster_phashes(
    phashes: &[(i64, [u8; 8])],
    near_dup_threshold: u32,
    related_threshold: u32,
) -> Vec<GroupResult> {
    let n = phashes.len();
    if n < 2 {
        return Vec::new();
    }

    let mut nd_uf = UnionFind::new(n);
    let mut rel_uf = UnionFind::new(n);

    for i in 0..n {
        for j in (i + 1)..n {
            let d = hamming_distance(&phashes[i].1, &phashes[j].1);
            if d <= near_dup_threshold {
                nd_uf.union(i, j);
                rel_uf.union(i, j);
            } else if d <= related_threshold {
                rel_uf.union(i, j);
            }
        }
    }

    let mut results = Vec::new();
    for component in rel_uf.components() {
        // Classify by near-dup cohesion: if all members share the same
        // near-dup root, the cluster is tight enough to call
        // "near_duplicate". Otherwise it's a broader "related" cluster.
        let mut nd_roots: std::collections::HashSet<usize> =
            std::collections::HashSet::new();
        for &idx in &component {
            nd_roots.insert(nd_uf.find(idx));
        }
        let group_type = if nd_roots.len() == 1 {
            "near_duplicate"
        } else {
            "related"
        };
        results.push(GroupResult {
            group_type,
            member_indices: component,
        });
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_cluster(phashes: &[(i64, [u8; 8])]) -> Vec<GroupResult> {
        cluster_phashes(phashes, DEFAULT_NEAR_DUP_THRESHOLD, DEFAULT_RELATED_THRESHOLD)
    }

    #[test]
    fn test_near_dup_cluster() {
        let phashes = vec![
            (1, [0x00; 8]),
            (2, [0x00; 8]),
            (3, [0xFF; 8]),
        ];
        let groups = default_cluster(&phashes);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].group_type, "near_duplicate");
        assert_eq!(groups[0].member_indices.len(), 2);
    }

    #[test]
    fn test_overlap_merges_into_single_related_group() {
        // Three photos: h0 and h1 are identical (near-dup), h2 is 8 bits
        // away — within the related threshold (12) but beyond the near-
        // dup threshold (4). With the old two-tier emit this produced
        // both a near-dup group {0,1} and a related group {0,1,2}. The
        // merged scheme emits a single "related" group containing all
        // three, so no photo lives in two groups at once.
        let h0 = [0x00u8; 8];
        let h1 = [0x00u8; 8];
        let mut h2 = [0x00u8; 8];
        h2[0] = 0xFF;

        let phashes = vec![(1, h0), (2, h1), (3, h2)];
        let groups = default_cluster(&phashes);

        assert_eq!(groups.len(), 1, "expected exactly one merged group");
        assert_eq!(groups[0].group_type, "related");
        assert_eq!(groups[0].member_indices.len(), 3);
    }

    #[test]
    fn test_every_photo_belongs_to_at_most_one_group() {
        // Regression guard for the user-reported "a photo appears in
        // two groups" bug. Constructs a cluster where the tight and
        // loose tiers overlap, then checks the invariant across the
        // full output.
        let h0 = [0x00u8; 8];
        let h1 = [0x00u8; 8];
        let mut h2 = [0x00u8; 8];
        h2[0] = 0x0F; // 4 bits — right at the near-dup threshold
        let mut h3 = [0x00u8; 8];
        h3[0] = 0xFF; // 8 bits — related only

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
    fn test_tighter_near_dup_threshold_splits_group() {
        // Two hashes at hamming distance 4 should group together with
        // default threshold=4, but split apart with threshold=2.
        let h0 = [0x00u8; 8];
        let mut h1 = [0x00u8; 8];
        h1[0] = 0x0F; // 4 bits different

        let phashes = vec![(1, h0), (2, h1)];

        let loose = cluster_phashes(&phashes, 4, 12);
        assert_eq!(loose.iter().filter(|g| g.group_type == "near_duplicate").count(), 1);

        let tight = cluster_phashes(&phashes, 2, 12);
        assert_eq!(tight.iter().filter(|g| g.group_type == "near_duplicate").count(), 0);
    }
}
