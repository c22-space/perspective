use std::collections::HashMap;
use uuid::Uuid;

/// Merge multiple ranked result lists using Reciprocal Rank Fusion.
///
/// Each list is a `Vec<(Uuid, score)>` ordered from most to least
/// relevant.  `k` is the RRF constant (typically 60.0).
///
/// The fused score for document `d` is:
///
///   Σ  1 / (k + rank_i(d))
///
/// where the sum runs over every list `i` that contains `d`.
pub fn rrf_fuse(result_sets: &[Vec<(Uuid, f32)>], k: f32) -> Vec<(Uuid, f32)> {
    let mut scores: HashMap<Uuid, f32> = HashMap::new();

    for results in result_sets {
        for (rank, (id, _score)) in results.iter().enumerate() {
            let contribution = 1.0 / (k + rank as f32 + 1.0);
            *scores.entry(*id).or_insert(0.0) += contribution;
        }
    }

    let mut fused: Vec<(Uuid, f32)> = scores.into_iter().collect();
    fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    fused
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_list_passthrough() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let list = vec![(a, 0.9), (b, 0.8)];
        let fused = rrf_fuse(&[list], 60.0);
        assert_eq!(fused.len(), 2);
        assert_eq!(fused[0].0, a);
    }

    #[test]
    fn two_lists_merge() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let list1 = vec![(a, 0.9), (b, 0.8), (c, 0.7)];
        let list2 = vec![(b, 0.85), (c, 0.75), (a, 0.65)];
        let fused = rrf_fuse(&[list1, list2], 60.0);
        assert_eq!(fused.len(), 3);
        // `b` is rank-1 in list2 and rank-2 in list1, while `a` is
        // rank-1 in list1 and rank-3 in list2, so `b` should win.
        assert_eq!(fused[0].0, b);
    }

    #[test]
    fn disjoint_lists() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let list1 = vec![(a, 0.9)];
        let list2 = vec![(b, 0.8)];
        let fused = rrf_fuse(&[list1, list2], 60.0);
        assert_eq!(fused.len(), 2);
    }
}
