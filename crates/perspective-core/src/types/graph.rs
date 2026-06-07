use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Types of graph nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GraphNode {
    MemoryRef {
        id: Uuid,
        memory_type: super::MemoryType,
    },
    Entity {
        id: Uuid,
        name: String,
        entity_type: EntityType,
    },
    Concept {
        id: Uuid,
        label: String,
    },
}

impl GraphNode {
    pub fn id(&self) -> Uuid {
        match self {
            GraphNode::MemoryRef { id, .. } => *id,
            GraphNode::Entity { id, .. } => *id,
            GraphNode::Concept { id, .. } => *id,
        }
    }
}

/// Entity categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Person,
    Organization,
    Concept,
    Tool,
    Project,
    Event,
    Location,
    Custom,
}

/// Types of graph edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    Temporal,
    Semantic,
    Entity,
    Causes,
    Enables,
    Supports,
    Contradicts,
    PromotedFrom,
}

/// A graph edge with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub from_id: Uuid,
    pub to_id: Uuid,
    pub edge_type: EdgeType,
    pub weight: f32,
    pub created_at: DateTime<Utc>,
    pub last_reinforced: DateTime<Utc>,
    pub decay_rate: f32,
}
