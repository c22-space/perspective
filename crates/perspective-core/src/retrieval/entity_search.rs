use crate::error::Result;
use crate::store::graph::GraphStore;
use crate::types::graph::GraphNode;
use std::collections::HashMap;
use uuid::Uuid;

/// Find all memory-reference node IDs that are linked to an entity whose
/// name matches `entity_name` (case-insensitive).
///
/// The search loads the full graph for `tenant_id` and walks edges from
/// matching entity nodes.
pub fn search_by_entity(
    store: &GraphStore,
    tenant_id: &str,
    entity_name: &str,
) -> Result<Vec<Uuid>> {
    let all_nodes = store.get_all_nodes(tenant_id)?;
    let all_edges = store.get_all_edges(tenant_id)?;

    let lower_name = entity_name.to_lowercase();

    // Collect IDs of entity nodes whose name matches.
    let entity_ids: Vec<Uuid> = all_nodes
        .iter()
        .filter_map(|node| match node {
            GraphNode::Entity { id, name, .. } if name.to_lowercase() == lower_name => Some(*id),
            _ => None,
        })
        .collect();

    // Collect all node IDs so we can check what an edge points to.
    let id_to_node: HashMap<Uuid, &GraphNode> = all_nodes.iter().map(|n| (n.id(), n)).collect();

    // For each entity node, find edges where it is the source and the
    // target is a MemoryRef.  Return those target IDs.
    let mut result_ids: Vec<Uuid> = Vec::new();
    for edge in &all_edges {
        if entity_ids.contains(&edge.from_id) {
            if let Some(target) = id_to_node.get(&edge.to_id) {
                if matches!(target, GraphNode::MemoryRef { .. }) {
                    result_ids.push(edge.to_id);
                }
            }
        }
    }

    result_ids.sort_unstable();
    result_ids.dedup();
    Ok(result_ids)
}
