# Perspective — Architecture

**A memory engine for AI agents.**

Graph + vector hybrid. Typed memory. LLM-powered extraction. Built in Rust.

---

## Core Principles

1. **No shortcuts.** Every decision optimizes for long-term quality, not shipping speed.
2. **Memory types matter.** Episodic, semantic, and procedural memories have different behaviors, decay rates, and retrieval strategies.
3. **Graph + vector, not graph or vector.** Vectors handle semantic retrieval. The graph handles relationships, consolidation, and entity-based queries.
4. **Decay is first-class.** Memories fade unless accessed. Ebbinghaus-style forgetting prevents unbounded accumulation.
5. **LLM extraction with cost control.** Smart batching and importance gating keep extraction quality high without per-turn LLM costs.
6. **Standalone engine.** Usable as an embedded library or client-server. First-class Hermes integration, but framework-agnostic.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                    Perspective Engine                     │
│                                                          │
│  ┌──────────┐  ┌──────────┐  ┌────────────────────────┐│
│  │  gRPC    │  │ Embedded │  │    Hermes Plugin        ││
│  │  Server  │  │  API     │  │  (MemoryProvider impl)  ││
│  └────┬─────┘  └────┬─────┘  └───────────┬────────────┘│
│       │              │                    │              │
│       └──────────────┴────────────────────┘              │
│                      │                                    │
│              ┌───────┴────────┐                          │
│              │  Core Engine   │                          │
│              │  (perspective) │                          │
│              └───────┬────────┘                          │
│                      │                                    │
│       ┌──────────────┼──────────────┐                    │
│       │              │              │                    │
│  ┌────┴────┐  ┌──────┴──────┐  ┌───┴────┐              │
│  │ Qdrant  │  │    redb     │  │  LLM   │              │
│  │(vectors)│  │ (graph/meta)│  │(extract)│              │
│  └─────────┘  └─────────────┘  └────────┘              │
└─────────────────────────────────────────────────────────┘
```

---

## Project Structure

```
perspective/
├── Cargo.toml                    # Workspace root
├── ARCHITECTURE.md
├── crates/
│   ├── perspective-core/         # Core engine library
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── engine.rs         # Main PerspectiveEngine struct
│   │   │   ├── types.rs          # Memory types, enums, structs
│   │   │   ├── config.rs         # Engine configuration
│   │   │   ├── error.rs          # Error types
│   │   │   ├── memory/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── episodic.rs   # Episodic memory behavior
│   │   │   │   ├── semantic.rs   # Semantic memory behavior
│   │   │   │   └── procedural.rs # Procedural memory behavior
│   │   │   ├── store/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── vector.rs     # Qdrant integration
│   │   │   │   ├── graph.rs      # redb + petgraph graph store
│   │   │   │   └── persistence.rs # Cross-store coordination
│   │   │   ├── extraction/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── pipeline.rs   # Extraction orchestration
│   │   │   │   ├── batcher.rs    # Smart batching + importance gate
│   │   │   │   ├── entities.rs   # Entity extraction (LLM + local NER)
│   │   │   │   └── relations.rs  # Relationship extraction
│   │   │   ├── retrieval/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── scorer.rs     # recency × importance × relevance
│   │   │   │   ├── vector_search.rs  # Qdrant vector retrieval
│   │   │   │   ├── graph_search.rs   # Graph traversal retrieval
│   │   │   │   ├── entity_search.rs  # Entity-based lookup
│   │   │   │   └── fusion.rs     # RRF fusion across retrieval methods
│   │   │   ├── decay/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── ebbinghaus.rs # Ebbinghaus forgetting curve
│   │   │   │   └── maintenance.rs # Background decay application
│   │   │   ├── consolidation/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── scheduler.rs  # Periodic consolidation trigger
│   │   │   │   ├── summarizer.rs # LLM-based memory compression
│   │   │   │   ├── promotion.rs  # Episodic → semantic promotion
│   │   │   │   ├── dedup.rs      # Duplicate detection + merge
│   │   │   │   └── communities.rs # Leiden community detection
│   │   │   └── embedding/
│   │   │       ├── mod.rs
│   │   │       ├── local.rs      # Local embedding model (fastembed)
│   │   │       └── api.rs        # External API embedding (OpenAI, etc.)
│   │   └── Cargo.toml
│   │
│   ├── perspective-server/       # gRPC server (client-server mode)
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── service.rs        # gRPC service implementation
│   │   │   ├── tenant.rs         # Tenant/collection management
│   │   │   └── health.rs         # Health checks, readiness
│   │   ├── proto/
│   │   │   └── perspective.proto # gRPC service definition
│   │   └── Cargo.toml
│   │
│   └── perspective-plugin/       # Hermes MemoryProvider plugin
│       ├── src/
│       │   ├── lib.rs
│       │   ├── provider.rs       # MemoryProvider trait impl
│       │   ├── session.rs        # Session lifecycle management
│       │   └── config.rs         # Plugin configuration
│       ├── plugin.yaml           # Hermes plugin manifest
│       └── Cargo.toml
│
├── proto/                        # Shared proto definitions
│   └── perspective.proto
│
├── tests/
│   ├── integration/
│   └── fixtures/
│
└── benchmarks/
    └── retrieval/
```

---

## Memory Types

### Episodic Memory
Specific events with temporal and contextual markers.

```
EpisodicMemory {
    id: UUID
    tenant_id: String
    content: String              # Raw event text
    embedding: Vec<f32>          # Vector representation
    timestamp: DateTime          # When it happened
    context: Option<String>      # Where/why it happened
    importance: f32              # LLM-scored 0.0-1.0
    access_count: u64
    last_accessed: DateTime
    stability: f32               # Ebbinghaus S parameter
    source_session: Option<String>
    tags: Vec<String>
    metadata: HashMap<String, Value>
}
```

**Behavior:**
- Created from raw observations and conversations
- High initial importance decay rate (fast forgetting)
- Stability increases with each access
- Promoted to semantic when accessed frequently enough
- Background consolidation summarizes clusters of related episodes

### Semantic Memory
Extracted facts and general knowledge.

```
SemanticMemory {
    id: UUID
    tenant_id: String
    content: String              # The fact/knowledge
    embedding: Vec<f32>
    confidence: f32              # Extraction confidence 0.0-1.0
    source_ids: Vec<UUID>        # Episodic memories that support this
    access_count: u64
    last_accessed: DateTime
    stability: f32               # High initial (slow decay)
    first_seen: DateTime
    last_validated: Option<DateTime>
    tags: Vec<String>
    metadata: HashMap<String, Value>
}
```

**Behavior:**
- Created by consolidation (episodic → semantic promotion) or direct extraction
- Low decay rate (facts persist longer)
- Sources are tracked for provenance
- Can be invalidated if contradicted by newer evidence
- Validated periodically against source memories

### Procedural Memory
Skills, patterns, and action sequences.

```
ProceduralMemory {
    id: UUID
    tenant_id: String
    content: String              # The procedure/skill description
    embedding: Vec<f32>
    code: Option<String>         # Optional executable code
    preconditions: Vec<String>   # When to use this
    postconditions: Vec<String>  # What it achieves
    success_rate: f32            # Track effectiveness
    access_count: u64
    last_used: DateTime
    stability: f32               # Very high (procedures persist)
    version: u32                 # Procedures can be refined
    tags: Vec<String>
    metadata: HashMap<String, Value>
}
```

**Behavior:**
- Created from successful action patterns
- No decay (procedures persist unless explicitly deprecated)
- Versioned (refined over time as better approaches are found)
- Success rate tracked to identify procedures that stop working

---

## Graph Model

The graph layer (redb + petgraph) tracks relationships between memories.

### Node Types
- `MemoryRef` — reference to any memory (episodic/semantic/procedural)
- `Entity` — named entity (person, concept, project, tool)
- `Concept` — abstract concept extracted during consolidation

### Edge Types
- `temporal` — memories close in time (weight = time proximity)
- `semantic` — memories with similar content (weight = cosine similarity)
- `entity` — memory mentions entity (weight = mention relevance)
- `causes` — causal relationship (weight = extraction confidence)
- `enables` — procedural dependency (weight = relevance)
- `supports` — episodic memory supports semantic fact (weight = evidence strength)
- `contradicts` — conflicting memories (weight = conflict strength)
- `promoted_from` — episodic memory promoted to semantic (weight = 1.0)

### Graph Properties
Every edge carries:
- `weight: f32` — strength of the relationship (0.0-1.0)
- `created_at: DateTime` — when the edge was created
- `last_reinforced: DateTime` — last time this edge was accessed/reinforced
- `decay_rate: f32` — how fast this edge weakens (type-dependent)

---

## Retrieval

The retrieval function: `score(memory, query) = recency × importance × relevance`

### Recency
Exponential time decay:
```
recency(memory) = e^(-λ × Δt)
where Δt = time since last access, λ = decay constant (type-specific)
```
- Episodic: λ = 0.1 (half-life ~7 days)
- Semantic: λ = 0.01 (half-life ~70 days)
- Procedural: λ = 0.0 (no decay)

### Importance
Node weight, set during extraction:
```
importance(memory) = base_score × access_boost
where base_score = LLM-scored (0.0-1.0)
      access_boost = min(1.0, 0.5 + 0.1 × log(access_count + 1))
```

### Relevance
Fusion of vector similarity and graph proximity:
```
relevance(memory, query) = max(vector_similarity, graph_proximity)
where vector_similarity = cosine(query_embedding, memory_embedding)
      graph_proximity = 1.0 / (1 + shortest_path_hops)
```

### Retrieval Pipeline
1. **Vector search**: Qdrant top-K by embedding similarity (over-fetch 5x)
2. **Entity search**: If query contains entities, find memories mentioning them via graph
3. **Graph expansion**: 1-hop from vector results via graph edges
4. **Fusion**: Reciprocal Rank Fusion across all result sets
5. **Scoring**: Apply recency × importance × relevance
6. **Budget**: Return top-N based on configured budget

---

## Extraction Pipeline

### Flow
```
Raw text arrives
    ↓
Importance gate (heuristic filter, free)
    ↓ (skip if unmemorable)
Buffer for batching
    ↓ (batch when N items or T seconds elapsed)
LLM extraction (single call per batch)
    ├── Entities (person, org, concept, tool)
    ├── Relationships (subject-predicate-object)
    ├── Facts (decomposed from long text)
    ├── Importance score (0.0-1.0)
    └── Memory type classification
    ↓
Entity resolution (local NER + fuzzy matching)
    ↓
Embedding generation (local or API)
    ↓
Store: Qdrant (vector) + redb (graph) + entity links
```

### Importance Gate (Heuristic)
Skip extraction for clearly unmemorable content:
- Very short messages (< 10 chars)
- Common acknowledgments ("ok", "thanks", "got it")
- System messages with no user content
- Exact duplicates of recent memories

### Smart Batching
- Buffer incoming memories for up to 30 seconds or 10 items
- Single LLM call extracts from entire batch
- Deduplicate within batch before extraction
- Cost: ~1 LLM call per 10 memories vs 1 per memory

---

## Decay System

### Ebbinghaus Forgetting Curve
Each memory has a `stability` parameter S:
```
strength(t) = e^(-t / S)
```

Stability increases with each access:
```
S_new = S_initial × (1 + α × access_count)
where α = learning rate (0.1 default)
```

Initial stability by type:
- Episodic: S = 1.0 (decays fast without access)
- Semantic: S = 10.0 (decays slowly)
- Procedural: S = ∞ (never decays)

### Thresholds
- **Retrieval threshold**: strength < 0.1 → excluded from retrieval results
- **Garbage collection threshold**: strength < 0.01 → eligible for deletion
- Deletion requires confirmation (background job flags, not auto-delete)

### Reinforcement
When a memory is accessed (retrieved or explicitly recalled):
1. Increment access_count
2. Update last_accessed
3. Increase stability: `S *= (1 + α)`
4. Reinforce connected edges (spreading activation boost)

---

## Consolidation System

### Trigger
Background scheduler runs consolidation at configurable intervals (default: every 4 hours).

### Phase 1: Deduplication
- Find memory pairs with cosine similarity > 0.95
- Merge into single memory, keeping the richer version
- Update all graph edges to point to merged memory
- Update source_ids for semantic memories

### Phase 2: Community Detection
- Build in-memory graph from redb snapshot (petgraph)
- Run Leiden algorithm for community detection
- Each community = cluster of related memories
- Generate community summary via LLM (one summary per community)

### Phase 3: Episodic → Semantic Promotion
- Find episodic memories accessed > N times (configurable, default 5)
- Extract the generalized knowledge from these episodes via LLM
- Create new semantic memory with the extracted fact
- Link original episodic memories as sources (`supports` edge)
- Original episodic memories get increased stability (rewarded)

### Phase 4: Staleness Detection
- Find semantic memories not accessed in > 30 days
- Check if source memories still exist and support the fact
- Flag unsupported facts for review
- Reduce confidence score of stale facts

### Phase 5: Contradiction Detection
- Find memory pairs with `contradicts` edge or high embedding similarity but different content
- Flag contradictions for review
- Keep both versions, mark newer one with higher confidence

---

## Embedding System

### Configuration
```toml
[embedding]
# Local model (default, zero-config)
provider = "local"
model = "all-MiniLM-L6-v2"  # ~80MB, 384 dimensions

# Or use an API
# provider = "openai"
# model = "text-embedding-3-small"
# api_key = "${OPENAI_API_KEY}"
```

### Requirements
- Any embedding provider must implement the `Embedder` trait
- Returns fixed-dimension vectors (dimension determined by model)
- Dimension is set at tenant creation time and cannot change
- All memories in a tenant must use the same embedding model

### Embedding Pipeline
- Raw text is augmented with metadata before embedding
- Format: `[type:episodic] {content} [tags:tag1,tag2] [date:2024-01-15]`
- This ensures retrieval queries can match on structure, not just content

---

## Multi-Tenancy

### Model
Collection-per-tenant. Each tenant gets:
- Dedicated Qdrant collection: `perspective_{tenant_id}`
- Dedicated redb namespace: `graph_{tenant_id}`
- Isolated entity resolution (no cross-tenant entity merging)
- Independent consolidation schedules

### Tenant Lifecycle
```
create_tenant(id, config) → creates Qdrant collection + redb namespace
delete_tenant(id) → drops collection + namespace + all data
list_tenants() → returns all active tenants
```

### Tenant Config
```toml
[tenant.defaults]
embedding_provider = "local"
embedding_model = "all-MiniLM-L6-v2"
consolidation_interval = "4h"
decay_enabled = true
max_memories = 100000
llm_extraction = true
llm_batch_size = 10
llm_batch_interval = "30s"
```

---

## gRPC API

```protobuf
service Perspective {
  // Memory operations
  rpc Store(StoreRequest) returns (StoreResponse);
  rpc Recall(RecallRequest) returns (RecallResponse);
  rpc GetMemory(GetMemoryRequest) returns (Memory);
  rpc UpdateMemory(UpdateMemoryRequest) returns (UpdateMemoryResponse);
  rpc DeleteMemory(DeleteMemoryRequest) returns (DeleteMemoryResponse);

  // Session management
  rpc StartSession(StartSessionRequest) returns (Session);
  rpc EndSession(EndSessionRequest) returns (EndSessionResponse);

  // Reflection
  rpc Reflect(ReflectRequest) returns (ReflectResponse);

  // Tenant management
  rpc CreateTenant(CreateTenantRequest) returns (Tenant);
  rpc DeleteTenant(DeleteTenantRequest) returns (DeleteTenantResponse);
  rpc ListTenants(ListTenantsRequest) returns (ListTenantsResponse);

  // Consolidation
  rpc TriggerConsolidation(ConsolidationRequest) returns (ConsolidationResponse);
  rpc GetConsolidationStatus(StatusRequest) returns (ConsolidationStatus);

  // Health
  rpc Health(HealthRequest) returns (HealthResponse);
}
```

---

## Hermes Plugin

Implements the `MemoryProvider` trait from Hermes Agent:

```rust
impl MemoryProvider for PerspectivePlugin {
    async fn retain(&self, content: RetainContent) -> Result<RetainResult>;
    async fn recall(&self, query: &str, budget: RecallBudget) -> Result<RecallResult>;
    async fn reflect(&self, query: &str, context: &[String]) -> Result<ReflectResult>;
    async fn session_start(&self, session_id: &str) -> Result<()>;
    async fn session_end(&self, session_id: &str) -> Result<()>;
    async fn health(&self) -> Result<HealthStatus>;
}
```

### Integration Points
- `retain`: Calls `Store` RPC with session metadata
- `recall`: Calls `Recall` RPC, formats for LLM context injection
- `reflect`: Calls `Reflect` RPC with LLM-powered synthesis
- Session lifecycle maps to Perspective session management
- Config via `plugin.yaml` in Hermes plugin directory

---

## Embedded Mode

When used as a Rust library:

```rust
use perspective::{PerspectiveEngine, Config};

let config = Config::default()
    .with_embedding(EmbeddingConfig::Local { model: "all-MiniLM-L6-v2".into() })
    .with_decay(DecayConfig::default())
    .with_consolidation(ConsolidationConfig::interval(Duration::from_secs(4 * 3600)));

let engine = PerspectiveEngine::new(config).await?;

// Store a memory
engine.store(StoreRequest {
    tenant_id: "user_bodmash".into(),
    content: "Bodmash prefers concise responses".into(),
    memory_type: MemoryType::Semantic,
    tags: vec!["preference".into()],
    ..Default::default()
}).await?;

// Recall relevant memories
let results = engine.recall(RecallRequest {
    tenant_id: "user_bodmash".into(),
    query: "How does Bodmash like responses?".into(),
    budget: 10,
    ..Default::default()
}).await?;
```

No Docker. No network. Single binary with embedded Qdrant and redb.

---

## Technology Stack

| Component | Choice | License | Why |
|-----------|--------|---------|-----|
| Vector DB | Qdrant | Apache 2.0 | Best vector search in Rust ecosystem |
| Graph store | redb | MIT | Embedded, simple, fast KV store |
| Graph algorithms | petgraph | MIT | In-memory graph algorithms |
| Full-text search | tantivy | MIT | BM25 scoring, tokenization |
| Embeddings (local) | fastembed-rs | MIT | Local ONNX inference |
| Serialization | serde + bincode | MIT | Fast binary serialization |
| gRPC | tonic | MIT | Rust gRPC framework |
| Runtime | tokio | MIT | Async runtime |
| CLI | clap | MIT | Argument parsing |

---

## Resolved Design Decisions

All architectural questions have been resolved:

1. **Qdrant embedded + external**: Embedded Qdrant for local/free use (single binary, zero dependencies). External Qdrant for hosted cloud (user connects to their own instance). Engine supports both via configuration.

2. **redb for graph persistence**: Simple, MIT, ACID, single-file database. Holds graph nodes, edges, weights, timestamps. petgraph runs algorithms on in-memory snapshots.

3. **Generic OpenAI-compatible LLM provider**: Any API following OpenAI chat completions format works (OpenAI, Ollama, vLLM, localai, etc.). Single implementation, user configures endpoint.

4. **Tantivy built now**: Full-text search (BM25) included from day one. Hybrid retrieval (vector + keyword + graph) is the foundation, not an afterthought.

5. **Flexible schema versioning**: Memories stored as JSON/protobuf with required fields. New fields are optional and backward compatible. No migration scripts needed. Engine reads any version, writes latest.

6. **Consolidation LLM prompts**: To be iterated during implementation. Quality depends heavily on prompt engineering. Dedicated iteration cycle during consolidation system build.
