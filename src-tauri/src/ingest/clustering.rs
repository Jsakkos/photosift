use super::phash::hamming_distance;

const NEAR_DUP_THRESHOLD: u32 = 4;
const RELATED_THRESHOLD: u32 = 12;

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

/// Two-tier disjoint clustering:
/// - Near-duplicate: hamming distance <= 4
/// - Related: hamming distance 5-12 (only if group spans multiple near-dup clusters)
pub fn cluster_phashes(phashes: &[(i64, [u8; 8])]) -> Vec<GroupResult> {
    let n = phashes.len();
    if n < 2 {
        return Vec::new();
    }

    let mut nd_uf = UnionFind::new(n);
    let mut rel_uf = UnionFind::new(n);

    for i in 0..n {
        for j in (i + 1)..n {
            let d = hamming_distance(&phashes[i].1, &phashes[j].1);
            if d <= NEAR_DUP_THRESHOLD {
                nd_uf.union(i, j);
                rel_uf.union(i, j);
            } else if d <= RELATED_THRESHOLD {
                rel_uf.union(i, j);
            }
        }
    }

    let mut results = Vec::new();

    // Emit near-duplicate groups
    for component in nd_uf.components() {
        results.push(GroupResult {
            group_type: "near_duplicate",
            member_indices: component,
        });
    }

    // Emit related groups that span multiple near-dup roots
    let rel_components = rel_uf.components();
    for component in rel_components {
        let mut nd_roots: std::collections::HashSet<usize> = std::collections::HashSet::new();
        for &idx in &component {
            nd_roots.insert(nd_uf.find(idx));
        }
        // Only emit if the related group spans at least two distinct near-dup clusters (or singletons)
        if nd_roots.len() >= 2 {
            results.push(GroupResult {
                group_type: "related",
                member_indices: component,
            });
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_near_dup_cluster() {
        // Three hashes: first two are identical (distance 0), third is far away
        let phashes = vec![
            (1, [0x00; 8]),
            (2, [0x00; 8]),
            (3, [0xFF; 8]),
        ];
        let groups = cluster_phashes(&phashes);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].group_type, "near_duplicate");
        assert_eq!(groups[0].member_indices.len(), 2);
    }

    #[test]
    fn test_two_tier_clustering() {
        // h0, h1: distance 0 (near-dup)
        // h2: distance ~8 from h0 (related, not near-dup)
        let h0 = [0x00u8; 8];
        let h1 = [0x00u8; 8];
        let mut h2 = [0x00u8; 8];
        h2[0] = 0xFF; // 8 bits different

        let phashes = vec![(1, h0), (2, h1), (3, h2)];
        let groups = cluster_phashes(&phashes);

        let nd_groups: Vec<_> = groups
            .iter()
            .filter(|g| g.group_type == "near_duplicate")
            .collect();
        let rel_groups: Vec<_> = groups
            .iter()
            .filter(|g| g.group_type == "related")
            .collect();

        assert_eq!(nd_groups.len(), 1);
        assert_eq!(nd_groups[0].member_indices.len(), 2);
        assert_eq!(rel_groups.len(), 1);
        assert_eq!(rel_groups[0].member_indices.len(), 3);
    }

    #[test]
    fn test_no_groups_for_distant_hashes() {
        let phashes = vec![
            (1, [0x00; 8]),
            (2, [0xFF; 8]),
        ];
        let groups = cluster_phashes(&phashes);
        assert!(groups.is_empty());
    }
}
