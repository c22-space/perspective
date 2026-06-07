use petgraph::graph::DiGraph;

use crate::types::{GraphEdge, GraphNode};

/// Detect communities in the perspective graph using connected-component
/// analysis on the undirected projection of the directed graph.
///
/// Each community is returned as a `Vec<usize>` of **petgraph node indices**.
/// Nodes that have no edges are placed in singleton communities.
pub fn detect_communities(graph: &DiGraph<GraphNode, GraphEdge>) -> Vec<Vec<usize>> {
    // Convert the directed graph to an undirected view for component analysis.
    let undirected = graph.clone();

    let mut visited = vec![false; undirected.node_count()];
    let mut communities = Vec::new();

    for node_idx in undirected.node_indices() {
        let idx = node_idx.index();
        if visited[idx] {
            continue;
        }

        // BFS to discover this connected component
        let mut component = Vec::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(node_idx);
        visited[idx] = true;

        while let Some(current) = queue.pop_front() {
            component.push(current.index());

            // Walk neighbours in both directions (treating as undirected)
            for neighbor in undirected.neighbors_undirected(current) {
                let nidx = neighbor.index();
                if !visited[nidx] {
                    visited[nidx] = true;
                    queue.push_back(neighbor);
                }
            }
        }

        communities.push(component);
    }

    communities
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::EdgeType;
    use chrono::Utc;

    fn make_node_ref(id: uuid::Uuid) -> GraphNode {
        GraphNode::MemoryRef {
            id,
            memory_type: crate::types::MemoryType::Episodic,
        }
    }

    #[test]
    fn test_single_component() {
        let mut graph = DiGraph::<GraphNode, GraphEdge>::new();
        let a = graph.add_node(make_node_ref(uuid::Uuid::new_v4()));
        let b = graph.add_node(make_node_ref(uuid::Uuid::new_v4()));
        graph.add_edge(
            a,
            b,
            GraphEdge {
                from_id: uuid::Uuid::new_v4(),
                to_id: uuid::Uuid::new_v4(),
                edge_type: EdgeType::Semantic,
                weight: 1.0,
                created_at: Utc::now(),
                last_reinforced: Utc::now(),
                decay_rate: 0.01,
            },
        );

        let communities = detect_communities(&graph);
        assert_eq!(communities.len(), 1);
        assert_eq!(communities[0].len(), 2);
    }

    #[test]
    fn test_two_components() {
        let mut graph = DiGraph::<GraphNode, GraphEdge>::new();
        let a = graph.add_node(make_node_ref(uuid::Uuid::new_v4()));
        let b = graph.add_node(make_node_ref(uuid::Uuid::new_v4()));
        let c = graph.add_node(make_node_ref(uuid::Uuid::new_v4()));
        let d = graph.add_node(make_node_ref(uuid::Uuid::new_v4()));

        graph.add_edge(
            a,
            b,
            GraphEdge {
                from_id: uuid::Uuid::new_v4(),
                to_id: uuid::Uuid::new_v4(),
                edge_type: EdgeType::Semantic,
                weight: 1.0,
                created_at: Utc::now(),
                last_reinforced: Utc::now(),
                decay_rate: 0.01,
            },
        );
        graph.add_edge(
            c,
            d,
            GraphEdge {
                from_id: uuid::Uuid::new_v4(),
                to_id: uuid::Uuid::new_v4(),
                edge_type: EdgeType::Temporal,
                weight: 1.0,
                created_at: Utc::now(),
                last_reinforced: Utc::now(),
                decay_rate: 0.01,
            },
        );

        let communities = detect_communities(&graph);
        assert_eq!(communities.len(), 2);

        // Each component should have 2 nodes
        let mut sizes: Vec<usize> = communities.iter().map(|c| c.len()).collect();
        sizes.sort();
        assert_eq!(sizes, vec![2, 2]);
    }

    #[test]
    fn test_singletons() {
        let mut graph = DiGraph::<GraphNode, GraphEdge>::new();
        let _a = graph.add_node(make_node_ref(uuid::Uuid::new_v4()));
        let _b = graph.add_node(make_node_ref(uuid::Uuid::new_v4()));

        let communities = detect_communities(&graph);
        assert_eq!(communities.len(), 2);
        assert!(communities.iter().all(|c| c.len() == 1));
    }
}
