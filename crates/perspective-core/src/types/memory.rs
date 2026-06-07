use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::collections::HashMap;

/// The three types of memory in Perspective.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    Episodic,
    Semantic,
    Procedural,
}

/// Base fields shared by all memory types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryBase {
    pub id: Uuid,
    pub tenant_id: String,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub tags: Vec<String>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Episodic memory - specific events with temporal/contextual markers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodicMemory {
    pub base: MemoryBase,
    pub timestamp: DateTime<Utc>,
    pub context: Option<String>,
    pub importance: f32,
    pub access_count: u64,
    pub last_accessed: DateTime<Utc>,
    pub stability: f32,
    pub source_session: Option<String>,
}

/// Semantic memory - extracted facts and general knowledge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticMemory {
    pub base: MemoryBase,
    pub confidence: f32,
    pub source_ids: Vec<Uuid>,
    pub access_count: u64,
    pub last_accessed: DateTime<Utc>,
    pub stability: f32,
    pub first_seen: DateTime<Utc>,
    pub last_validated: Option<DateTime<Utc>>,
}

/// Procedural memory - skills, patterns, and action sequences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProceduralMemory {
    pub base: MemoryBase,
    pub code: Option<String>,
    pub preconditions: Vec<String>,
    pub postconditions: Vec<String>,
    pub success_rate: f32,
    pub access_count: u64,
    pub last_used: DateTime<Utc>,
    pub stability: f32,
    pub version: u32,
}

/// Unified memory enum for storage and retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Memory {
    #[serde(rename = "episodic")]
    Episodic(EpisodicMemory),
    #[serde(rename = "semantic")]
    Semantic(SemanticMemory),
    #[serde(rename = "procedural")]
    Procedural(ProceduralMemory),
}

impl Memory {
    pub fn id(&self) -> Uuid {
        match self {
            Memory::Episodic(m) => m.base.id,
            Memory::Semantic(m) => m.base.id,
            Memory::Procedural(m) => m.base.id,
        }
    }

    pub fn tenant_id(&self) -> &str {
        match self {
            Memory::Episodic(m) => &m.base.tenant_id,
            Memory::Semantic(m) => &m.base.tenant_id,
            Memory::Procedural(m) => &m.base.tenant_id,
        }
    }

    pub fn content(&self) -> &str {
        match self {
            Memory::Episodic(m) => &m.base.content,
            Memory::Semantic(m) => &m.base.content,
            Memory::Procedural(m) => &m.base.content,
        }
    }

    pub fn memory_type(&self) -> MemoryType {
        match self {
            Memory::Episodic(_) => MemoryType::Episodic,
            Memory::Semantic(_) => MemoryType::Semantic,
            Memory::Procedural(_) => MemoryType::Procedural,
        }
    }

    pub fn access_count(&self) -> u64 {
        match self {
            Memory::Episodic(m) => m.access_count,
            Memory::Semantic(m) => m.access_count,
            Memory::Procedural(m) => m.access_count,
        }
    }

    pub fn stability(&self) -> f32 {
        match self {
            Memory::Episodic(m) => m.stability,
            Memory::Semantic(m) => m.stability,
            Memory::Procedural(m) => m.stability,
        }
    }

    /// Returns the timestamp of the last access (read or use) for this memory.
    pub fn last_accessed(&self) -> DateTime<Utc> {
        match self {
            Memory::Episodic(m) => m.last_accessed,
            Memory::Semantic(m) => m.last_accessed,
            Memory::Procedural(m) => m.last_used,
        }
    }
}
