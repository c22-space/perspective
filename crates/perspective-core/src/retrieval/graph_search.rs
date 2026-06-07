use crate::error::Result;
use crate::store::graph::GraphStore;
use std::collections::{HashMap, VecDeque};
use uuid::Uuid;

/// BFS-expand neighbours up to `hops` hops from `node_id`.
///
/// Each discovered node is returned as `(node_id, score)` where the score
/// is `1.0 / (1.0 + depth)` – closer nodes score higher.  Nodes reached
/// via multiple paths keep the highest (closest) score.
pub fn expand_neighbors(
    store: &GraphStore,
    tenant_id: &str,
    node_id: Uuid,
    hops: u32,
) -> Result<Vec<(Uuid, f32)>> {
    let mut visited: HashMap<Uuid, f32> = HashMap::new();
    let mut queue: VecDeque<(Uuid, u32)> = VecDeque::new();

    visited.insert(node_id, 1.0); // seed score, will not be in output
    queue.push_back((node_id, 0));

    while let Some((current, depth)) = queue.pop_front() {
        if depth >= hops {
            continue;
        }
        let neighbours = store.get_neighbors(tenant_id, current, None)?;
        for (target_node, _edge) in neighbours {
            let target_id = target_node.id();
            let score = 1.0 / (1.0 + (depth + 1) as f32);
            match visited.get(&target_id) {
                Some(&existing) if existing >= score => {}
                _ => {
                    visited.insert(target_id, score);
                    queue.push_back((target_id, depth + 1));
                }
            }
        }
    }

    // Exclude the seed node from output
    let results: Vec<(Uuid, f32)> = visited
        .into_iter()
        .filter(|(id, _)| *id != node_id)
        .collect();

    Ok(results)
}
