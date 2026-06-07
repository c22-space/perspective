# Perspective — Implementation Plan

> **For Hermes:** Use subagent-driven-development skill to implement this plan task-by-task.

**Goal:** Build the Perspective memory engine — a graph+vector hybrid memory system for AI agents, written in Rust.

**Architecture:** Workspace with 3 crates (core, server, plugin). Core handles all memory operations. Server wraps core in gRPC. Plugin wraps core for Hermes integration. Storage: Qdrant (vectors) + redb (graph) + Tantivy (BM25). Typed memory (episodic/semantic/procedural) with Ebbinghaus decay and periodic consolidation.

**Tech Stack:** Rust, tokio, tonic (gRPC), qdrant-client, redb, petgraph, tantivy, fastembed, serde, uuid, chrono

---

## Phase 0: Foundation

### Task 0.1: Workspace setup

**Objective:** Create Cargo workspace with all three crates

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/perspective-core/Cargo.toml`
- Create: `crates/perspective-server/Cargo.toml`
- Create: `crates/perspective-plugin/Cargo.toml`
- Create: `crates/perspective-core/src/lib.rs`
- Create: `crates/perspective-server/src/main.rs`
- Create: `crates/perspective-plugin/src/lib.rs`

**Step 1:** Create workspace root Cargo.toml

```toml
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"
repository = "https://github.com/c22-space/perspective"

[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
thiserror = "2"
tracing = "0.1"
tracing-subscriber = "0.3"
```

**Step 2:** Create each crate's Cargo.toml with dependencies

**Step 3:** Create minimal lib.rs/main.rs with `pub mod` stubs

**Step 4:** Verify build

Run: `cargo check 2>&1`
Expected: `Finished` with no errors

**Step 5:** Commit

```bash
git add -A && git commit -m "feat: workspace setup with 3 crates"
```

---

### Task 0.2: Core types — Memory type system

**Objective:** Define the memory types that are the foundation of the entire engine

**Files:**
- Create: `crates/perspective-core/src/types/mod.rs`
- Create: `crates/perspective-core/src/types/memory.rs`
- Create: `crates/perspective-core/src/types/mod.rs`

**Step 1:** Create `MemoryType` enum and memory structs

```rust
// crates/perspective-core/src/types/memory.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::collections::HashMap;

/// The three types of memory in Perspective.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    /// Specific events with temporal/contextual markers
    Episodic,
    /// Extracted facts and general knowledge
    Semantic,
    /// Skills, patterns, and action sequences
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

/// Episodic memory — specific events.
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

/// Semantic memory — extracted facts.
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

/// Procedural memory — skills and patterns.
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
```

**Step 2:** Create module file with re-exports

```rust
// crates/perspective-core/src/types/mod.rs
pub mod memory;
pub use memory::*;
```

**Step 3:** Update lib.rs

```rust
// crates/perspective-core/src/lib.rs
pub mod types;
```

**Step 4:** Verify build

Run: `cargo check -p perspective-core 2>&1`
Expected: `Finished` with no errors

**Step 5:** Commit

```bash
git add crates/perspective-core/ && git commit -m "feat: core memory types (episodic, semantic, procedural)"
```

---

### Task 0.3: Core types — Graph model

**Objective:** Define graph nodes and edges for the knowledge graph

**Files:**
- Create: `crates/perspective-core/src/types/graph.rs`

**Step 1:** Create graph types

```rust
// crates/perspective-core/src/types/graph.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Types of graph nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum GraphNode {
    /// Reference to a memory (any type)
    MemoryRef {
        id: Uuid,
        memory_type: super::MemoryType,
    },
    /// Named entity (person, org, concept, tool)
    Entity {
        id: Uuid,
        name: String,
        entity_type: EntityType,
    },
    /// Abstract concept from consolidation
    Concept {
        id: Uuid,
        label: String,
    },
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
    /// Memories close in time
    Temporal,
    /// Similar content
    Semantic,
    /// Memory mentions entity
    Entity,
    /// Causal relationship
    Causes,
    /// Procedural dependency
    Enables,
    /// Episodic supports semantic fact
    Supports,
    /// Conflicting memories
    Contradicts,
    /// Episodic promoted to semantic
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
```

**Step 2:** Add to types/mod.rs

```rust
pub mod graph;
pub use graph::*;
```

**Step 3:** Verify build and commit

```bash
cargo check -p perspective-core 2>&1
git add crates/perspective-core/ && git commit -m "feat: graph model (nodes, edges, entity types)"
```

---

### Task 0.4: Core types — Configuration

**Objective:** Define engine configuration

**Files:**
- Create: `crates/perspective-core/src/config.rs`

**Step 1:** Create config structs

```rust
// crates/perspective-core/src/config.rs
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Main engine configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub embedding: EmbeddingConfig,
    pub extraction: ExtractionConfig,
    pub decay: DecayConfig,
    pub consolidation: ConsolidationConfig,
    pub storage: StorageConfig,
    pub retrieval: RetrievalConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "provider")]
pub enum EmbeddingConfig {
    #[serde(rename = "local")]
    Local { model: String },
    #[serde(rename = "api")]
    Api {
        endpoint: String,
        model: String,
        api_key: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionConfig {
    pub enabled: bool,
    pub endpoint: String,
    pub model: String,
    pub api_key: Option<String>,
    pub batch_size: usize,
    pub batch_interval_secs: u64,
    pub importance_gate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecayConfig {
    pub enabled: bool,
    pub episodic_lambda: f32,
    pub semantic_lambda: f32,
    pub procedural_lambda: f32,
    pub learning_rate: f32,
    pub retrieval_threshold: f32,
    pub gc_threshold: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationConfig {
    pub enabled: bool,
    pub interval_secs: u64,
    pub dedup_similarity_threshold: f32,
    pub promotion_access_count: u64,
    pub staleness_days: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub qdrant_url: Option<String>,
    pub qdrant_api_key: Option<String>,
    pub embedded_qdrant: bool,
    pub data_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalConfig {
    pub default_budget: usize,
    pub vector_overfetch: usize,
    pub graph_hop_limit: usize,
    pub rrf_k: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            embedding: EmbeddingConfig::Local {
                model: "all-MiniLM-L6-v2".into(),
            },
            extraction: ExtractionConfig {
                enabled: true,
                endpoint: "http://localhost:11434/v1".into(),
                model: "llama3".into(),
                api_key: None,
                batch_size: 10,
                batch_interval_secs: 30,
                importance_gate: true,
            },
            decay: DecayConfig {
                enabled: true,
                episodic_lambda: 0.1,
                semantic_lambda: 0.01,
                procedural_lambda: 0.0,
                learning_rate: 0.1,
                retrieval_threshold: 0.1,
                gc_threshold: 0.01,
            },
            consolidation: ConsolidationConfig {
                enabled: true,
                interval_secs: 4 * 3600,
                dedup_similarity_threshold: 0.95,
                promotion_access_count: 5,
                staleness_days: 30,
            },
            storage: StorageConfig {
                qdrant_url: None,
                qdrant_api_key: None,
                embedded_qdrant: true,
                data_dir: PathBuf::from("./perspective-data"),
            },
            retrieval: RetrievalConfig {
                default_budget: 10,
                vector_overfetch: 5,
                graph_hop_limit: 2,
                rrf_k: 60.0,
            },
        }
    }
}
```

**Step 2:** Add to lib.rs and verify

```rust
pub mod types;
pub mod config;
```

**Step 3:** Commit

```bash
git add crates/perspective-core/ && git commit -m "feat: engine configuration with defaults"
```

---

### Task 0.5: Core types — Error types

**Objective:** Define error types for the engine

**Files:**
- Create: `crates/perspective-core/src/error.rs`

**Step 1:** Create error enum

```rust
// crates/perspective-core/src/error.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PerspectiveError {
    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Qdrant error: {0}")]
    Qdrant(String),

    #[error("Graph error: {0}")]
    Graph(String),

    #[error("Extraction error: {0}")]
    Extraction(String),

    #[error("Embedding error: {0}")]
    Embedding(String),

    #[error("Retrieval error: {0}")]
    Retrieval(String),

    #[error("Tenant not found: {0}")]
    TenantNotFound(String),

    #[error("Memory not found: {0}")]
    MemoryNotFound(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("LLM API error: {0}")]
    LlmApi(String),
}

pub type Result<T> = std::result::Result<T, PerspectiveError>;
```

**Step 2:** Update lib.rs

```rust
pub mod types;
pub mod config;
pub mod error;
```

**Step 3:** Commit

```bash
git add crates/perspective-core/ && git commit -m "feat: error types"
```

---

## Phase 1: Storage Layer

### Task 1.1: Embedding module — Local embeddings

**Objective:** Implement local embedding via fastembed

**Files:**
- Create: `crates/perspective-core/src/embedding/mod.rs`
- Create: `crates/perspective-core/src/embedding/local.rs`

**Step 1:** Add fastembed dependency to core Cargo.toml

```toml
[dependencies]
fastembed = "4"
```

**Step 2:** Create Embedder trait and local implementation

```rust
// crates/perspective-core/src/embedding/mod.rs
pub mod local;

use async_trait::async_trait;
use crate::error::Result;

#[async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn dimensions(&self) -> usize;
    fn model_name(&self) -> &str;
}
```

```rust
// crates/perspective-core/src/embedding/local.rs
use async_trait::async_trait;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use super::Embedder;
use crate::error::{PerspectiveError, Result};

pub struct LocalEmbedder {
    model: TextEmbedding,
    dimensions: usize,
    model_name: String,
}

impl LocalEmbedder {
    pub fn new(model_name: &str) -> Result<Self> {
        let model = match model_name {
            "all-MiniLM-L6-v2" => TextEmbedding::try_new(
                InitOptions::new(EmbeddingModel::AllMiniLmL6V2),
            ),
            "all-mpnet-base-v2" => TextEmbedding::try_new(
                InitOptions::new(EmbeddingModel::AllMpnetBaseV2),
            ),
            _ => return Err(PerspectiveError::Embedding(
                format!("Unknown model: {}", model_name)
            )),
        }.map_err(|e| PerspectiveError::Embedding(e.to_string()))?;

        let dimensions = model.dimensions();
        Ok(Self { model, dimensions, model_name: model_name.into() })
    }
}

#[async_trait]
impl Embedder for LocalEmbedder {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let embeddings = self.model
            .embed(texts.to_vec(), None)
            .map_err(|e| PerspectiveError::Embedding(e.to_string()))?;
        Ok(embeddings)
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }
}
```

**Step 3:** Update lib.rs

```rust
pub mod types;
pub mod config;
pub mod error;
pub mod embedding;
```

**Step 4:** Verify build

Run: `cargo check -p perspective-core 2>&1`

**Step 5:** Commit

```bash
git add crates/perspective-core/ && git commit -m "feat: local embedding via fastembed"
```

---

### Task 1.2: Vector store — Qdrant integration

**Objective:** Implement vector storage with Qdrant

**Files:**
- Create: `crates/perspective-core/src/store/mod.rs`
- Create: `crates/perspective-core/src/store/vector.rs`

**Step 1:** Add qdrant-client dependency

```toml
qdrant-client = "1"
```

**Step 2:** Create VectorStore trait and Qdrant implementation

```rust
// crates/perspective-core/src/store/vector.rs
use qdrant_client::Qdrant;
use qdrant_client::qdrant::{
    CreateCollectionBuilder, Distance, PointStruct, VectorParamsBuilder,
    Filter, FieldCondition, MatchValue,
};
use uuid::Uuid;
use crate::error::{PerspectiveError, Result};

pub struct QdrantVectorStore {
    client: Qdrant,
    collection_prefix: String,
}

impl QdrantVectorStore {
    pub async fn new(url: &str, api_key: Option<&str>) -> Result<Self> {
        let client = Qdrant::from_url(url)
            .api_key(api_key.unwrap_or(""))
            .build()
            .map_err(|e| PerspectiveError::Qdrant(e.to_string()))?;
        Ok(Self {
            client,
            collection_prefix: "perspective".into(),
        })
    }

    fn collection_name(&self, tenant_id: &str) -> String {
        format!("{}_{}", self.collection_prefix, tenant_id)
    }

    pub async fn ensure_collection(
        &self,
        tenant_id: &str,
        dimensions: u64,
    ) -> Result<()> {
        let name = self.collection_name(tenant_id);
        self.client
            .create_collection(
                CreateCollectionBuilder::new(&name)
                    .vectors_config(VectorParamsBuilder::new(dimensions, Distance::Cosine)),
            )
            .await
            .map_err(|e| PerspectiveError::Qdrant(e.to_string()))?;
        Ok(())
    }

    pub async fn upsert(
        &self,
        tenant_id: &str,
        id: Uuid,
        vector: Vec<f32>,
        payload: serde_json::Value,
    ) -> Result<()> {
        let name = self.collection_name(tenant_id);
        let point = PointStruct::new(id.to_string(), vector, payload);
        self.client
            .upsert_points(name, vec![point], None)
            .await
            .map_err(|e| PerspectiveError::Qdrant(e.to_string()))?;
        Ok(())
    }

    pub async fn search(
        &self,
        tenant_id: &str,
        query_vector: Vec<f32>,
        limit: u64,
    ) -> Result<Vec<SearchResult>> {
        let name = self.collection_name(tenant_id);
        let results = self.client
            .query_points(
                qdrant_client::qdrant::QueryPointsBuilder::new(&name)
                    .query(query_vector)
                    .limit(limit)
                    .with_payload(true),
            )
            .await
            .map_err(|e| PerspectiveError::Qdrant(e.to_string()))?;

        Ok(results
            .into_iter()
            .map(|r| SearchResult {
                id: Uuid::parse_str(&r.id.to_string()).unwrap_or_default(),
                score: r.score,
                payload: r.payload.map(|p| serde_json::to_value(p).unwrap_or_default()),
            })
            .collect())
    }

    pub async fn delete(&self, tenant_id: &str, id: Uuid) -> Result<()> {
        let name = self.collection_name(tenant_id);
        self.client
            .delete_points(
                name.clone(),
                qdrant_client::qdrant::PointsSelector::Points(
                    qdrant_client::qdrant::PointsIdsList {
                        ids: vec![id.to_string().into()],
                    },
                ),
                None,
            )
            .await
            .map_err(|e| PerspectiveError::Qdrant(e.to_string()))?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct SearchResult {
    pub id: Uuid,
    pub score: f32,
    pub payload: Option<serde_json::Value>,
}
```

**Step 3:** Create store/mod.rs

```rust
pub mod vector;
```

**Step 4:** Update lib.rs

```rust
pub mod store;
```

**Step 5:** Verify and commit

```bash
cargo check -p perspective-core 2>&1
git add crates/perspective-core/ && git commit -m "feat: Qdrant vector store integration"
```

---

### Task 1.3: Graph store — redb integration

**Objective:** Implement graph persistence with redb + petgraph

**Files:**
- Create: `crates/perspective-core/src/store/graph.rs`

**Step 1:** Add dependencies

```toml
redb = "2"
petgraph = "0.6"
```

**Step 2:** Create GraphStore

```rust
// crates/perspective-core/src/store/graph.rs
use redb::{Database, TableDefinition};
use petgraph::graph::{DiGraph, NodeIndex};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::path::Path;
use std::collections::HashMap;
use crate::types::graph::{GraphNode, GraphEdge, EdgeType, EntityType};
use crate::error::{PerspectiveError, Result};

const NODES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("nodes");
const EDGES_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("edges");

#[derive(Serialize, Deserialize)]
struct StoredNode {
    id: String,
    node: GraphNode,
}

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
        let db = Database::create(path)
            .map_err(|e| PerspectiveError::Graph(e.to_string()))?;
        Ok(Self { db })
    }

    /// Load graph into petgraph for in-memory operations.
    pub fn load_graph(&self, tenant_id: &str) -> Result<DiGraph<GraphNode, GraphEdge>> {
        let read = self.db.begin_read()
            .map_err(|e| PerspectiveError::Graph(e.to_string()))?;

        let mut graph = DiGraph::new();
        let mut id_to_index: HashMap<String, NodeIndex> = HashMap::new();

        // Load nodes
        if let Ok(table) = read.open_table(NODES_TABLE) {
            for entry in table.iter() {
                let entry = entry.map_err(|e| PerspectiveError::Graph(e.to_string()))?;
                let stored: StoredNode = bincode::deserialize(entry.value())
                    .map_err(|e| PerspectiveError::Graph(e.to_string()))?;
                if stored.id.starts_with(tenant_id) {
                    let idx = graph.add_node(stored.node);
                    id_to_index.insert(stored.id, idx);
                }
            }
        }

        // Load edges
        if let Ok(table) = read.open_table(EDGES_TABLE) {
            for entry in table.iter() {
                let entry = entry.map_err(|e| PerspectiveError::Graph(e.to_string()))?;
                let stored: StoredEdge = bincode::deserialize(entry.value())
                    .map_err(|e| PerspectiveError::Graph(e.to_string()))?;
                if let (Some(&from_idx), Some(&to_idx)) = (
                    id_to_index.get(&stored.from_id),
                    id_to_index.get(&stored.to_id),
                ) {
                    graph.add_edge(from_idx, to_idx, stored.edge);
                }
            }
        }

        Ok(graph)
    }

    /// Save a node to the graph store.
    pub fn save_node(&self, node: &GraphNode) -> Result<()> {
        let write = self.db.begin_write()
            .map_err(|e| PerspectiveError::Graph(e.to_string()))?;
        {
            let mut table = write.open_table(NODES_TABLE)
                .map_err(|e| PerspectiveError::Graph(e.to_string()))?;
            let id = match node {
                GraphNode::MemoryRef { id, .. } => id.to_string(),
                GraphNode::Entity { id, .. } => id.to_string(),
                GraphNode::Concept { id, .. } => id.to_string(),
            };
            let stored = StoredNode { id: id.clone(), node: node.clone() };
            let bytes = bincode::serialize(&stored)
                .map_err(|e| PerspectiveError::Graph(e.to_string()))?;
            table.insert(id.as_str(), bytes.as_slice())
                .map_err(|e| PerspectiveError::Graph(e.to_string()))?;
        }
        write.commit()
            .map_err(|e| PerspectiveError::Graph(e.to_string()))?;
        Ok(())
    }

    /// Save an edge to the graph store.
    pub fn save_edge(&self, edge: &GraphEdge) -> Result<()> {
        let write = self.db.begin_write()
            .map_err(|e| PerspectiveError::Graph(e.to_string()))?;
        {
            let mut table = write.open_table(EDGES_TABLE)
                .map_err(|e| PerspectiveError::Graph(e.to_string()))?;
            let key = format!("{}:{}", edge.from_id, edge.to_id);
            let stored = StoredEdge {
                from_id: edge.from_id.to_string(),
                to_id: edge.to_id.to_string(),
                edge: edge.clone(),
            };
            let bytes = bincode::serialize(&stored)
                .map_err(|e| PerspectiveError::Graph(e.to_string()))?;
            table.insert(key.as_bytes(), bytes.as_slice())
                .map_err(|e| PerspectiveError::Graph(e.to_string()))?;
        }
        write.commit()
            .map_err(|e| PerspectiveError::Graph(e.to_string()))?;
        Ok(())
    }

    /// Get all neighbors of a node (1-hop).
    pub fn get_neighbors(
        &self,
        tenant_id: &str,
        node_id: Uuid,
        edge_type: Option<EdgeType>,
    ) -> Result<Vec<(GraphNode, GraphEdge)>> {
        let graph = self.load_graph(tenant_id)?;
        let mut results = Vec::new();

        // Find the node index
        for node_idx in graph.node_indices() {
            let node = &graph[node_idx];
            let id = match node {
                GraphNode::MemoryRef { id, .. } => *id,
                GraphNode::Entity { id, .. } => *id,
                GraphNode::Concept { id, .. } => *id,
            };
            if id == node_id {
                // Get outgoing edges
                for edge_idx in graph.edges(node_idx) {
                    let edge = edge_idx.weight();
                    if edge_type.map_or(true, |et| edge.edge_type == et) {
                        let target = &graph[edge_idx.target()];
                        results.push((target.clone(), edge.clone()));
                    }
                }
                break;
            }
        }

        Ok(results)
    }
}
```

**Step 3:** Add to store/mod.rs and lib.rs

**Step 4:** Verify and commit

```bash
cargo check -p perspective-core 2>&1
git add crates/perspective-core/ && git commit -m "feat: redb + petgraph graph store"
```

---

### Task 1.4: Full-text search — Tantivy integration

**Objective:** Implement BM25 keyword search

**Files:**
- Create: `crates/perspective-core/src/store/text.rs`

**Step 1:** Add tantivy dependency

```toml
tantivy = "0.22"
tantivy-jieba = "0.10"  # or whatever tokenizer
```

**Step 2:** Create TextStore

```rust
// crates/perspective-core/src/store/text.rs
use tantivy::schema::*;
use tantivy::{Index, IndexReader, IndexWriter, doc};
use tantivy::query::QueryParser;
use uuid::Uuid;
use std::path::Path;
use crate::error::{PerspectiveError, Result};

pub struct TextStore {
    index: Index,
    reader: IndexReader,
    schema: Schema,
    content_field: Field,
    id_field: Field,
    tenant_field: Field,
}

#[derive(Debug)]
pub struct TextSearchResult {
    pub id: Uuid,
    pub score: f32,
}

impl TextStore {
    pub fn new(path: &Path) -> Result<Self> {
        let mut schema_builder = Schema::builder();
        let content_field = schema_builder.add_text_field("content", TEXT | STORED);
        let id_field = schema_builder.add_text_field("id", STRING | STORED);
        let tenant_field = schema_builder.add_text_field("tenant", STRING | STORED);
        let schema = schema_builder.build();

        let index = Index::create_in_dir(path, schema.clone())
            .map_err(|e| PerspectiveError::Storage(e.to_string()))?;

        let reader = index
            .reader_builder()
            .reload_policy(tantivy::ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e| PerspectiveError::Storage(e.to_string()))?;

        Ok(Self {
            index,
            reader,
            schema,
            content_field,
            id_field,
            tenant_field,
        })
    }

    pub fn add_document(
        &self,
        tenant_id: &str,
        id: Uuid,
        content: &str,
    ) -> Result<()> {
        let mut writer: IndexWriter = self.index
            .writer(50_000_000) // 50MB heap
            .map_err(|e| PerspectiveError::Storage(e.to_string()))?;

        let doc = doc!(
            self.content_field => content,
            self.id_field => id.to_string().as_str(),
            self.tenant_field => tenant_id,
        );

        writer.add_document(doc)
            .map_err(|e| PerspectiveError::Storage(e.to_string()))?;
        writer.commit()
            .map_err(|e| PerspectiveError::Storage(e.to_string()))?;
        Ok(())
    }

    pub fn search(
        &self,
        tenant_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<TextSearchResult>> {
        let searcher = self.reader.searcher();

        let mut query_parser = QueryParser::for_index(
            &self.index,
            vec![self.content_field],
        );
        query_parser.add_whitelisted_term(self.tenant_field);

        let parsed_query = query_parser
            .parse_query(query)
            .map_err(|e| PerspectiveError::Storage(e.to_string()))?;

        let top_docs = searcher
            .search(&parsed_query, &tantivy::collector::TopDocs::with_limit(limit))
            .map_err(|e| PerspectiveError::Storage(e.to_string()))?;

        let mut results = Vec::new();
        for (score, doc_addr) in top_docs {
            if let Ok(doc) = searcher.doc::<tantivy::TantivyDocument>(doc_addr) {
                if let Some(id_val) = doc.get_first(self.id_field) {
                    if let Some(id_str) = id_val.as_str() {
                        if id_val.as_str().map(|s| s == tenant_id).unwrap_or(true) {
                            if let Ok(id) = Uuid::parse_str(id_str) {
                                results.push(TextSearchResult { id, score });
                            }
                        }
                    }
                }
            }
        }

        Ok(results)
    }
}
```

**Step 3:** Update store/mod.rs and lib.rs

**Step 4:** Verify and commit

```bash
cargo check -p perspective-core 2>&1
git add crates/perspective-core/ && git commit -m "feat: Tantivy full-text search (BM25)"
```

---

## Phase 2: Engine Core

### Task 2.1: PerspectiveEngine — Main struct

**Objective:** Create the main engine struct that ties everything together

**Files:**
- Create: `crates/perspective-core/src/engine.rs`

**Step 1:** Create engine with store handles

```rust
// crates/perspective-core/src/engine.rs
use std::sync::Arc;
use crate::config::Config;
use crate::error::Result;
use crate::store::vector::QdrantVectorStore;
use crate::store::graph::GraphStore;
use crate::store::text::TextStore;
use crate::embedding::local::LocalEmbedder;
use crate::embedding::Embedder;

pub struct PerspectiveEngine {
    config: Config,
    vector_store: Arc<QdrantVectorStore>,
    graph_store: Arc<GraphStore>,
    text_store: Arc<TextStore>,
    embedder: Arc<dyn Embedder>,
}

impl PerspectiveEngine {
    pub async fn new(config: Config) -> Result<Self> {
        // Initialize vector store
        let vector_store = if let Some(ref url) = config.storage.qdrant_url {
            QdrantVectorStore::new(url, config.storage.qdrant_api_key.as_deref()).await?
        } else {
            // Embedded Qdrant (to be implemented)
            QdrantVectorStore::new("http://localhost:6334", None).await?
        };

        // Initialize graph store
        let graph_path = config.storage.data_dir.join("graph.redb");
        std::fs::create_dir_all(&config.storage.data_dir)?;
        let graph_store = GraphStore::new(&graph_path)?;

        // Initialize text store
        let text_path = config.storage.data_dir.join("tantivy");
        std::fs::create_dir_all(&text_path)?;
        let text_store = TextStore::new(&text_path)?;

        // Initialize embedder
        let embedder: Arc<dyn Embedder> = match &config.embedding {
            crate::config::EmbeddingConfig::Local { model } => {
                Arc::new(LocalEmbedder::new(model)?)
            }
            _ => return Err(crate::error::PerspectiveError::Config(
                "API embeddings not yet implemented".into(),
            )),
        };

        Ok(Self {
            config,
            vector_store: Arc::new(vector_store),
            graph_store: Arc::new(graph_store),
            text_store: Arc::new(text_store),
            embedder,
        })
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn embedder(&self) -> &dyn Embedder {
        self.embedder.as_ref()
    }
}
```

**Step 2:** Add `async-trait` dependency

```toml
async-trait = "0.1"
```

**Step 3:** Verify and commit

```bash
cargo check -p perspective-core 2>&1
git add crates/perspective-core/ && git commit -m "feat: PerspectiveEngine main struct"
```

---

## Phase 3: Hermes Plugin

### Task 3.1: Plugin structure

**Objective:** Create the Hermes MemoryProvider plugin

**Files:**
- Create: `crates/perspective-plugin/plugin.yaml`
- Create: `crates/perspective-plugin/src/provider.rs`
- Create: `crates/perspective-plugin/src/config.rs`

**Step 1:** Create plugin.yaml

```yaml
name: perspective
version: 0.1.0
description: "Perspective memory engine for Hermes"
author: c22
license: MIT
memory_provider: true
```

**Step 2:** Create MemoryProvider implementation stub

```rust
// crates/perspective-plugin/src/provider.rs
use perspective_core::engine::PerspectiveEngine;
use perspective_core::config::Config;

pub struct PerspectiveProvider {
    engine: PerspectiveEngine,
}

impl PerspectiveProvider {
    pub async fn new(config: Config) -> perspective_core::error::Result<Self> {
        let engine = PerspectiveEngine::new(config).await?;
        Ok(Self { engine })
    }

    pub async fn retain(&self, content: &str, session_id: &str, metadata: serde_json::Value) -> perspective_core::error::Result<String> {
        // TODO: implement retain logic
        Ok("retained".into())
    }

    pub async fn recall(&self, query: &str, budget: usize) -> perspective_core::error::Result<String> {
        // TODO: implement recall logic
        Ok("".into())
    }

    pub async fn reflect(&self, query: &str) -> perspective_core::error::Result<String> {
        // TODO: implement reflect logic
        Ok("".into())
    }
}
```

**Step 3:** Update plugin lib.rs

```rust
pub mod provider;
pub mod config;
```

**Step 4:** Verify and commit

```bash
cargo check -p perspective-plugin 2>&1
git add crates/perspective-plugin/ && git commit -m "feat: Hermes plugin structure"
```

---

## Phase 4: Dashboard

### Task 4.1: AGENTS.md

**Objective:** Add project guidance for AI agents

**Files:**
- Create: `AGENTS.md`

**Step 1:** Create AGENTS.md with project context

```markdown
# Perspective — Agent Guidelines

## Project Overview
Perspective is a graph+vector memory engine for AI agents, written in Rust.
MIT license. Standalone engine with first-class Hermes integration.

## Architecture
- Workspace with 3 crates: perspective-core, perspective-server, perspective-plugin
- Storage: Qdrant (vectors) + redb (graph) + Tantivy (BM25)
- Memory types: episodic, semantic, procedural
- LLM extraction via generic OpenAI-compatible API
- Ebbinghaus decay, periodic consolidation

## Build Commands
- `cargo check` — verify compilation
- `cargo test` — run tests
- `cargo build` — full build
- `cargo clippy` — lint

## Code Style
- Rust 2021 edition
- `thiserror` for error types
- `serde` for serialization
- `async-trait` for async traits
- `tracing` for logging
- No `unwrap()` in production code, use `?` or `map_err`

## Testing
- Unit tests in each module
- Integration tests in `tests/`
- Use `#[tokio::test]` for async tests

## Key Files
- `crates/perspective-core/src/types/` — Memory type definitions
- `crates/perspective-core/src/engine.rs` — Main engine struct
- `crates/perspective-core/src/store/` — Storage layer
- `crates/perspective-core/src/retrieval/` — Retrieval pipeline
- `crates/perspective-plugin/` — Hermes integration
- `ARCHITECTURE.md` — Full architecture document
```

**Step 2:** Commit

```bash
git add AGENTS.md && git commit -m "feat: AGENTS.md project guidelines"
```

---

## Phase 5: Dashboard

### Task 5.1: CLI status command

**Objective:** `perspective status` command for quick health checks

**Files:**
- Add to: `crates/perspective-server/src/main.rs` (clap CLI)

**Features:**
- `perspective status` — tenant count, memory counts by type, last consolidation, health
- `perspective status --tenant <id>` — per-tenant detail
- `perspective status --json` — machine-readable output
- Color-coded health indicators (green/yellow/red)

### Task 5.2: Web dashboard

**Objective:** Lightweight web dashboard for deeper visibility

**Files:**
- Create: `crates/perspective-dashboard/` (new crate, or embedded in server)

**Features:**
- Memory stats: total counts by type, growth over time
- Tenant overview: per-tenant memory counts, health
- Consolidation status: last run, duration, memories consolidated
- Decay metrics: memories approaching threshold, GC candidates
- Retrieval stats: queries per minute, avg latency, recall quality
- Entity graph visualization (simple: top entities by connection count)
- Real-time updates via SSE or polling
- Lightweight: single HTML file with embedded JS (no npm/build step)

**Tech:** Axum or Warp for serving, single HTML file with Chart.js or similar

### Task 5.3: Dashboard spec

**Objective:** Detailed spec for dashboard data endpoints

**gRPC endpoints for dashboard data:**
- `GetStats` — overall engine statistics
- `GetTenantStats` — per-tenant breakdown
- `GetConsolidationStatus` — consolidation job details
- `GetDecayStatus` — memory decay metrics
- `StreamEvents` — SSE/WebSocket for real-time updates

---

## Phase 6: Specs (TBD after core implementation)

Specs will be added as components are built:
- Retrieval scoring spec
- Decay curve spec
- Consolidation pipeline spec
- gRPC API spec
- Dashboard spec

---

## Implementation Order

Phases 0-1 are the foundation (types + storage). Phase 2 ties them together. Phase 3 (Hermes plugin) can start as soon as the engine has store/recall. Phase 4 (AGENTS.md) is done. Dashboard specs will follow.

**Next immediate tasks after foundation:**
1. Retrieval pipeline (vector + graph + BM25 + fusion)
2. Extraction pipeline (batching + LLM)
3. Decay system
4. Consolidation system
5. gRPC server
6. Hermes plugin (full implementation)
7. Dashboard
