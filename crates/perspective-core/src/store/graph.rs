use crate::error::{PerspectiveError, Result};
use crate::types::graph::{EdgeType, GraphEdge, GraphNode};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

const NODES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("nodes");
const EDGES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("edges");

#[derive(Serialize, Deserialize)]
struct StoredEdge {
    from_id: String,
    to_id: String,
    edge: GraphEdge,
}

pub struct GraphStore {
    db: Database,
}

impl GraphStore {
    pub fn new(path: &Path) -> Result<Self> {
        std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))
            .map_err(|e| PerspectiveError::Graph(e.to_string()))?;
        let db = Database::create(path).map_err(|e| PerspectiveError::Graph(e.to_string()))?;
        Ok(Self { db })
    }

    /// Load graph into petgraph for in-memory operations.
    pub fn load_graph(&self, tenant_id: &str) -> Result<DiGraph<GraphNode, GraphEdge>> {
        let read = self
            .db
            .begin_read()
            .map_err(|e| PerspectiveError::Graph(e.to_string()))?;

        let mut graph = DiGraph::new();
        let mut id_to_index: HashMap<String, NodeIndex> = HashMap::new();

        // Load nodes
        if let Ok(table) = read.open_table(NODES_TABLE) {
            for entry in table
                .iter()
                .map_err(|e| PerspectiveError::Graph(e.to_string()))?
            {
                let (key, value) = entry.map_err(|e| PerspectiveError::Graph(e.to_string()))?;
                let key = key.value().to_string();
                if key.starts_with(tenant_id) {
                    if let Ok(node) = bincode::deserialize::<GraphNode>(value.value()) {
                        let idx = graph.add_node(node);
                        id_to_index.insert(key, idx);
                    }
                }
            }
        }

        // Load edges
        if let Ok(table) = read.open_table(EDGES_TABLE) {
            for entry in table
                .iter()
                .map_err(|e| PerspectiveError::Graph(e.to_string()))?
            {
                let (_key, value) = entry.map_err(|e| PerspectiveError::Graph(e.to_string()))?;
                if let Ok(stored) = bincode::deserialize::<StoredEdge>(value.value()) {
                    if let (Some(&from_idx), Some(&to_idx)) = (
                        id_to_index.get(&stored.from_id),
                        id_to_index.get(&stored.to_id),
                    ) {
                        graph.add_edge(from_idx, to_idx, stored.edge);
                    }
                }
            }
        }

        Ok(graph)
    }

    /// Save a node to the graph store.
    pub fn save_node(&self, tenant_id: &str, node: &GraphNode) -> Result<()> {
        let write = self
            .db
            .begin_write()
            .map_err(|e| PerspectiveError::Graph(e.to_string()))?;
        {
            let mut table = write
                .open_table(NODES_TABLE)
                .map_err(|e| PerspectiveError::Graph(e.to_string()))?;
            let key = format!("{}:{}", tenant_id, node.id());
            let bytes =
                bincode::serialize(node).map_err(|e| PerspectiveError::Graph(e.to_string()))?;
            table
                .insert(key.as_str(), bytes.as_slice())
                .map_err(|e| PerspectiveError::Graph(e.to_string()))?;
        }
        write
            .commit()
            .map_err(|e| PerspectiveError::Graph(e.to_string()))?;
        Ok(())
    }

    /// Save an edge to the graph store.
    pub fn save_edge(&self, tenant_id: &str, edge: &GraphEdge) -> Result<()> {
        let write = self
            .db
            .begin_write()
            .map_err(|e| PerspectiveError::Graph(e.to_string()))?;
        {
            let mut table = write
                .open_table(EDGES_TABLE)
                .map_err(|e| PerspectiveError::Graph(e.to_string()))?;
            let key = format!("{}:{}:{}", tenant_id, edge.from_id, edge.to_id);
            let stored = StoredEdge {
                from_id: edge.from_id.to_string(),
                to_id: edge.to_id.to_string(),
                edge: edge.clone(),
            };
            let bytes =
                bincode::serialize(&stored).map_err(|e| PerspectiveError::Graph(e.to_string()))?;
            table
                .insert(key.as_str(), bytes.as_slice())
                .map_err(|e| PerspectiveError::Graph(e.to_string()))?;
        }
        write
            .commit()
            .map_err(|e| PerspectiveError::Graph(e.to_string()))?;
        Ok(())
    }

    /// Get 1-hop neighbors of a node.
    pub fn get_neighbors(
        &self,
        tenant_id: &str,
        node_id: Uuid,
        edge_type: Option<EdgeType>,
    ) -> Result<Vec<(GraphNode, GraphEdge)>> {
        let graph = self.load_graph(tenant_id)?;
        let mut results = Vec::new();

        for node_idx in graph.node_indices() {
            let node = &graph[node_idx];
            let id = node.id();
            if id == node_id {
                for edge_ref in graph.edges(node_idx) {
                    let edge = edge_ref.weight();
                    if edge_type.as_ref().is_none_or(|et| edge.edge_type == *et) {
                        let target = &graph[edge_ref.target()];
                        results.push((target.clone(), edge.clone()));
                    }
                }
                break;
            }
        }

        Ok(results)
    }

    /// Get all nodes in a tenant's graph.
    pub fn get_all_nodes(&self, tenant_id: &str) -> Result<Vec<GraphNode>> {
        let graph = self.load_graph(tenant_id)?;
        Ok(graph.node_weights().cloned().collect())
    }

    /// Get all edges in a tenant's graph.
    pub fn get_all_edges(&self, tenant_id: &str) -> Result<Vec<GraphEdge>> {
        let graph = self.load_graph(tenant_id)?;
        Ok(graph.edge_weights().cloned().collect())
    }

    /// Find an Entity or Concept node by name within a tenant.
    pub fn find_entity_by_name(&self, tenant_id: &str, name: &str) -> Result<Option<GraphNode>> {
        let graph = self.load_graph(tenant_id)?;
        let lower = name.to_lowercase();
        for node in graph.node_weights() {
            match node {
                GraphNode::Entity { name: n, .. } | GraphNode::Concept { label: n, .. } => {
                    if n.to_lowercase() == lower {
                        return Ok(Some(node.clone()));
                    }
                }
                _ => {}
            }
        }
        Ok(None)
    }

    /// Find or create an Entity node by name. Returns (node_id, is_new).
    pub fn upsert_entity(
        &self,
        tenant_id: &str,
        name: &str,
        entity_type: crate::types::EntityType,
    ) -> Result<(Uuid, bool)> {
        if let Some(existing) = self.find_entity_by_name(tenant_id, name)? {
            return Ok((existing.id(), false));
        }
        let id = Uuid::new_v4();
        let node = GraphNode::Entity {
            id,
            name: name.to_string(),
            entity_type,
        };
        self.save_node(tenant_id, &node)?;
        Ok((id, true))
    }

    /// Find or create a Concept node by label. Returns (node_id, is_new).
    pub fn upsert_concept(&self, tenant_id: &str, label: &str) -> Result<(Uuid, bool)> {
        if let Some(existing) = self.find_entity_by_name(tenant_id, label)? {
            return Ok((existing.id(), false));
        }
        let id = Uuid::new_v4();
        let node = GraphNode::Concept {
            id,
            label: label.to_string(),
        };
        self.save_node(tenant_id, &node)?;
        Ok((id, true))
    }

    /// Check if an edge already exists between two nodes.
    pub fn edge_exists(&self, tenant_id: &str, from_id: Uuid, to_id: Uuid) -> Result<bool> {
        let graph = self.load_graph(tenant_id)?;
        let from_str = from_id.to_string();
        let to_str = to_id.to_string();
        for node_idx in graph.node_indices() {
            let node = &graph[node_idx];
            if node.id().to_string() == from_str {
                for edge_ref in graph.edges(node_idx) {
                    let target = &graph[edge_ref.target()];
                    if target.id().to_string() == to_str {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }

    /// Save an edge only if it doesn't already exist.
    pub fn save_edge_if_new(&self, tenant_id: &str, edge: &GraphEdge) -> Result<()> {
        if self.edge_exists(tenant_id, edge.from_id, edge.to_id)? {
            return Ok(());
        }
        self.save_edge(tenant_id, edge)
    }

    /// Count all nodes and edges across all tenants. Returns (node_count, edge_count, nodes, edges).
    pub fn count_all(&self) -> Result<(u64, u64, Vec<GraphNode>, Vec<GraphEdge>)> {
        let txn = self
            .db
            .begin_read()
            .map_err(|e| PerspectiveError::Graph(e.to_string()))?;

        let mut all_nodes = Vec::new();
        if let Ok(table) = txn.open_table(NODES_TABLE) {
            let iter = table
                .iter()
                .map_err(|e| PerspectiveError::Graph(e.to_string()))?;
            for item in iter {
                let (_, val) = item.map_err(|e| PerspectiveError::Graph(e.to_string()))?;
                let data: GraphNode = bincode::deserialize(val.value())
                    .map_err(|e| PerspectiveError::Graph(e.to_string()))?;
                all_nodes.push(data);
            }
        }

        let mut all_edges = Vec::new();
        if let Ok(table) = txn.open_table(EDGES_TABLE) {
            let iter = table
                .iter()
                .map_err(|e| PerspectiveError::Graph(e.to_string()))?;
            for item in iter {
                let (_, val) = item.map_err(|e| PerspectiveError::Graph(e.to_string()))?;
                let stored: StoredEdge = bincode::deserialize(val.value())
                    .map_err(|e| PerspectiveError::Graph(e.to_string()))?;
                all_edges.push(stored.edge);
            }
        }

        let node_count = all_nodes.len() as u64;
        let edge_count = all_edges.len() as u64;
        Ok((node_count, edge_count, all_nodes, all_edges))
    }
}
