use crate::config::Config;
use crate::consolidation::communities::detect_communities;
use crate::consolidation::dedup::find_duplicates;
use crate::consolidation::promotion::find_promotable;
use crate::embedding::Embedder;
use crate::embedding::LocalEmbedder;
use crate::error::{PerspectiveError, Result};
use crate::extraction::batcher::ExtractionBatcher;
use crate::extraction::entities::extract_entities;
use crate::extraction::pipeline::ExtractionPipeline;
use crate::extraction::relations::extract_relations;
use crate::monitor::*;
use crate::store::graph::GraphStore;
use crate::store::text::TextStore;
use crate::store::vector::QdrantVectorStore;
use crate::types::*;
use chrono::{DateTime, Utc};
use crate::decay::ebbinghaus::reinforce;
use crate::decay::maintenance::memory_strength;
use crate::retrieval::fusion::rrf_fuse;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub struct PerspectiveEngine {
    pub config: std::sync::RwLock<Config>,
    vector_store: Mutex<QdrantVectorStore>,
    graph_store: Option<Arc<GraphStore>>,
    text_store: Arc<TextStore>,
    embedder: Arc<dyn Embedder>,
    pub monitor: Arc<Monitor>,
    extraction_pipeline: Option<Arc<ExtractionPipeline>>,
    batcher: Mutex<ExtractionBatcher>,
    stores_since_last_consolidation: std::sync::atomic::AtomicU64,
    pending_consolidation: std::sync::atomic::AtomicBool,
}

#[derive(Debug, Clone)]
pub struct RecallResult {
    pub memories: Vec<Memory>,
    pub scores: Vec<f32>,
}

#[derive(Debug, Clone)]
pub struct StoreRequest {
    pub tenant_id: String,
    pub content: String,
    pub memory_type: MemoryType,
    pub tags: Vec<String>,
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
    pub context: Option<String>,
    pub source_session: Option<String>,
    /// When true, skip extraction (used for internally-generated memories).
    pub skip_extraction: bool,
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConsolidationReport {
    pub duplicates_found: usize,
    pub promotable_count: usize,
    pub communities: usize,
}

impl PerspectiveEngine {
    pub fn new(config: Config) -> Result<Self> {
        // Initialize embedder
        let embedder: Arc<dyn Embedder> = match &config.embedding {
            crate::config::EmbeddingConfig::Local { model } => Arc::new(LocalEmbedder::new(model)?),
            crate::config::EmbeddingConfig::Api {
                endpoint,
                model,
                api_key: _,
            } => {
                return Err(PerspectiveError::Config(format!(
                    "API embeddings not yet implemented (endpoint: {}, model: {})",
                    endpoint, model
                )));
            }
        };

        // Initialize vector store (embedded, no Docker needed)
        let qdrant_path = config.storage.data_dir.join("qdrant");
        let vector_store = Mutex::new(QdrantVectorStore::new(&qdrant_path)?);

        // Initialize graph store
        let graph_path = config.storage.data_dir.join("graph.redb");
        let graph_store = Arc::new(GraphStore::new(&graph_path)?);

        // Initialize text store
        let text_path = config.storage.data_dir.join("tantivy");
        let text_store = Arc::new(TextStore::new(&text_path)?);

        let monitor = Arc::new(Monitor::new(&config.storage.data_dir));

        // Initialize extraction pipeline
        let extraction_pipeline = if config.extraction.enabled {
            Some(Arc::new(ExtractionPipeline::new(config.extraction.clone())))
        } else {
            None
        };
        let batcher = ExtractionBatcher::new(&config.extraction);

        Ok(Self {
            config: std::sync::RwLock::new(config),
            vector_store,
            graph_store: Some(graph_store),
            text_store,
            embedder,
            monitor,
            extraction_pipeline,
            batcher: Mutex::new(batcher),
            stores_since_last_consolidation: std::sync::atomic::AtomicU64::new(0),
            pending_consolidation: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// Create engine for dashboard read-only mode.
    /// Skips graph store if redb is locked by another process.
    pub fn new_readonly(config: Config) -> Result<Self> {
        let embedder: Arc<dyn Embedder> = match &config.embedding {
            crate::config::EmbeddingConfig::Local { model } => Arc::new(LocalEmbedder::new(model)?),
            crate::config::EmbeddingConfig::Api {
                endpoint,
                model,
                api_key: _,
            } => {
                return Err(PerspectiveError::Config(format!(
                    "API embeddings not yet implemented (endpoint: {}, model: {})",
                    endpoint, model
                )));
            }
        };

        let qdrant_path = config.storage.data_dir.join("qdrant");
        let vector_store = Mutex::new(QdrantVectorStore::new(&qdrant_path)?);

        // Skip graph store if redb is locked by another process (e.g. Hermes plugin)
        let graph_path = config.storage.data_dir.join("graph.redb");
        let graph_store = match GraphStore::new(&graph_path) {
            Ok(gs) => Some(Arc::new(gs)),
            Err(e) => {
                eprintln!("  ⚠ Graph store unavailable (locked?): {e}. Dashboard will show graph as empty.");
                None
            }
        };

        let text_path = config.storage.data_dir.join("tantivy");
        let text_store = Arc::new(TextStore::new(&text_path)?);

        let monitor = Arc::new(Monitor::new(&config.storage.data_dir));

        let extraction_pipeline = if config.extraction.enabled {
            Some(Arc::new(ExtractionPipeline::new(config.extraction.clone())))
        } else {
            None
        };
        let batcher = ExtractionBatcher::new(&config.extraction);

        Ok(Self {
            config: std::sync::RwLock::new(config),
            vector_store,
            graph_store,
            text_store,
            embedder,
            monitor,
            extraction_pipeline,
            batcher: Mutex::new(batcher),
            stores_since_last_consolidation: std::sync::atomic::AtomicU64::new(0),
            pending_consolidation: std::sync::atomic::AtomicBool::new(false),
        })
    }

    pub fn config(&self) -> std::sync::RwLockReadGuard<'_, Config> {
        self.config.read().unwrap()
    }

    pub fn embedder(&self) -> &dyn Embedder {
        self.embedder.as_ref()
    }

    /// Store a memory in all three stores.
    pub async fn store(&self, req: StoreRequest) -> Result<Uuid> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        tracing::info!(
            "store: tenant={} type={} content_preview=\"{}\"",
            req.tenant_id,
            req.memory_type,
            &req.content.chars().take(80).collect::<String>(),
        );

        // Generate embedding (blocking call for local model)
        let embed_start = std::time::Instant::now();
        let embedding_vecs = self
            .embedder
            .embed(&[&req.content])
            .await
            .map_err(|e| PerspectiveError::Embedding(e.to_string()))?;
        tracing::debug!("store: embedding generated in {:?} ({})", embed_start.elapsed(), embedding_vecs.len());
        let embedding = embedding_vecs
            .into_iter()
            .next()
            .ok_or_else(|| PerspectiveError::Embedding("No embedding returned".into()))?;

        // Create memory based on type
        let memory = match req.memory_type {
            MemoryType::Episodic => Memory::Episodic(EpisodicMemory {
                base: MemoryBase {
                    id,
                    tenant_id: req.tenant_id.clone(),
                    content: req.content.clone(),
                    embedding: Some(embedding.clone()),
                    tags: req.tags.clone(),
                    metadata: req.metadata.clone(),
                    created_at: now,
                    updated_at: now,
                },
                timestamp: now,
                context: req.context,
                importance: 0.5,
                access_count: 0,
                last_accessed: now,
                stability: 1.0,
                source_session: req.source_session,
            }),
            MemoryType::Semantic => Memory::Semantic(SemanticMemory {
                base: MemoryBase {
                    id,
                    tenant_id: req.tenant_id.clone(),
                    content: req.content.clone(),
                    embedding: Some(embedding.clone()),
                    tags: req.tags.clone(),
                    metadata: req.metadata.clone(),
                    created_at: now,
                    updated_at: now,
                },
                confidence: 0.8,
                source_ids: vec![],
                access_count: 0,
                last_accessed: now,
                stability: 10.0,
                first_seen: now,
                last_validated: None,
            }),
            MemoryType::Procedural => Memory::Procedural(ProceduralMemory {
                base: MemoryBase {
                    id,
                    tenant_id: req.tenant_id.clone(),
                    content: req.content.clone(),
                    embedding: Some(embedding.clone()),
                    tags: req.tags.clone(),
                    metadata: req.metadata.clone(),
                    created_at: now,
                    updated_at: now,
                },
                code: None,
                preconditions: vec![],
                postconditions: vec![],
                success_rate: 1.0,
                access_count: 0,
                last_used: now,
                stability: f32::INFINITY,
                version: 1,
            }),
        };

        // Store in Qdrant (embedded)
        let payload = serde_json::json!({
            "tenant_id": req.tenant_id,
            "memory_type": memory.memory_type(),
            "content": req.content,
            "tags": req.tags,
            "created_at": now.to_rfc3339(),
        });

        let dims = self.embedder.dimensions();
        self.vector_store
            .lock()
            .map_err(|e| PerspectiveError::Qdrant(e.to_string()))?
            .upsert(&req.tenant_id, id, embedding, payload, dims)?;
        tracing::debug!("store: vector upsert id={id} tenant={}", req.tenant_id);

        // Store in graph (skip if unavailable)
        if let Some(ref gs) = self.graph_store {
            gs.save_node(
                &req.tenant_id,
                &GraphNode::MemoryRef {
                    id,
                    memory_type: req.memory_type,
                },
            )?;

            // Create temporal edge to most recent existing memory
            if let Ok(all_nodes) = gs.get_all_nodes(&req.tenant_id) {
                if let Some(last_node) = all_nodes.iter().find(|n| n.id() != id) {
                    let edge = GraphEdge {
                        from_id: id,
                        to_id: last_node.id(),
                        edge_type: EdgeType::Temporal,
                        weight: 0.8,
                        created_at: now,
                        last_reinforced: now,
                        decay_rate: self.config().decay.episodic_lambda,
                    };
                    gs.save_edge(&req.tenant_id, &edge)?;
                }
            }
        }

        // Store in text index
        self.text_store
            .add_document(&req.tenant_id, id, &req.content, &req.memory_type.to_string())?;
        tracing::debug!("store: text index updated id={id}");

        self.monitor.record_event(
            "store",
            Some(&req.memory_type.to_string()),
            Some(&req.content),
            true,
            Some(&serde_json::json!({"content": req.content, "memory_type": req.memory_type.to_string(), "tags": req.tags}).to_string()),
        );

        // --- Async extraction: entities, relations, LLM facts ---
        // Local extraction (fast, no network) runs inline.
        // LLM extraction is buffered and processed by start_extraction_loop().
        // Skip extraction for internally-generated memories (extracted facts).
        if !req.skip_extraction {
            if let Some(ref gs) = self.graph_store {
                let content = req.content.clone();
                let tenant = req.tenant_id.clone();
                let memory_id = id;

                // 1. Extract entities locally
                let entities = extract_entities(&content);
                tracing::debug!("store: extracted {} entities", entities.len());

                // 2. Extract relations locally
                let relations = extract_relations(&content, &entities);
                tracing::debug!("store: extracted {} relations", relations.len());

                // 3. Create Entity/Concept nodes + edges in graph
                for ent in &entities {
                    if let Ok((entity_id, _is_new)) =
                        gs.upsert_entity(&tenant, &ent.name, ent.entity_type)
                    {
                        // Link memory -> entity
                        let edge = GraphEdge {
                            from_id: memory_id,
                            to_id: entity_id,
                            edge_type: EdgeType::Entity,
                            weight: ent.confidence,
                            created_at: now,
                            last_reinforced: now,
                            decay_rate: 0.01,
                        };
                        let _ = gs.save_edge_if_new(&tenant, &edge);
                    }
                }

                // 4. Create relation edges
                for rel in &relations {
                    // Find or create subject entity
                    if let Ok((subj_id, _)) =
                        gs.upsert_entity(&tenant, &rel.subject, crate::types::EntityType::Custom)
                    {
                        // Find or create object entity
                        if let Ok((obj_id, _)) =
                            gs.upsert_entity(&tenant, &rel.object, crate::types::EntityType::Custom)
                        {
                            let edge = GraphEdge {
                                from_id: subj_id,
                                to_id: obj_id,
                                edge_type: EdgeType::Semantic,
                                weight: rel.confidence,
                                created_at: now,
                                last_reinforced: now,
                                decay_rate: 0.01,
                            };
                            let _ = gs.save_edge_if_new(&tenant, &edge);
                        }
                    }
                }

                self.monitor.record_event(
                    "extraction",
                    Some("local"),
                    Some(&format!(
                        "{} entities, {} relations",
                        entities.len(),
                        relations.len()
                    )),
                    true,
                    Some(&serde_json::json!({"entities": entities.len(), "relations": relations.len()}).to_string()),
                );
            }

            // 5. Buffer for LLM extraction (processed by start_extraction_loop)
            if self.config().extraction.enabled && self.extraction_pipeline.is_some() {
                let pipeline = self.extraction_pipeline.as_ref().unwrap();
                if pipeline.is_memorable(&req.content) {
                    if let Ok(mut batcher) = self.batcher.lock() {
                        batcher.buffer(&req.tenant_id, &req.content);
                        tracing::debug!("store: buffered for LLM extraction, queue_len={}", batcher.len());
                    }
                }
            }
        } // end skip_extraction guard

        tracing::info!("store: complete id={id} tenant={}", req.tenant_id);

        // Trigger consolidation every 100 memories
        let count = self.stores_since_last_consolidation.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        if count >= 100 {
            self.stores_since_last_consolidation.store(0, std::sync::atomic::Ordering::Relaxed);
            self.pending_consolidation.store(true, std::sync::atomic::Ordering::Relaxed);
            tracing::info!("consolidation: flagged (100 stores reached)");
        }

        Ok(id)
    }

    /// Recall relevant memories for a query.
    pub async fn recall(
        &self,
        tenant_id: &str,
        query: &str,
        budget: usize,
    ) -> Result<RecallResult> {
        tracing::info!(
            "recall: tenant={tenant_id} query=\"{}\" budget={budget}",
            &query.chars().take(80).collect::<String>(),
        );
        let recall_start = std::time::Instant::now();
        let _now = Utc::now();
        let overfetch = budget * self.config().retrieval.vector_overfetch;

        // 1. Vector search (embedded)
        let query_embedding = self
            .embedder
            .embed(&[query])
            .await
            .map_err(|e| PerspectiveError::Embedding(e.to_string()))?
            .into_iter()
            .next()
            .ok_or_else(|| PerspectiveError::Embedding("No embedding returned".into()))?;

        let vector_results = self
            .vector_store
            .lock()
            .map_err(|e| PerspectiveError::Qdrant(e.to_string()))?
            .search(
                tenant_id,
                query_embedding,
                overfetch,
                self.embedder.dimensions(),
            )?;
        tracing::debug!("recall: vector search returned {} results", vector_results.len());

        // 2. Text search (BM25)
        let text_results = self.text_store.search(tenant_id, query, overfetch)?;
        tracing::debug!("recall: text search returned {} results", text_results.len());

        // 3. Reciprocal Rank Fusion via shared fusion module
        let vector_tuples: Vec<(Uuid, f32)> = vector_results.iter().map(|r| (r.id, r.score)).collect();
        let text_tuples: Vec<(Uuid, f32)> = text_results.iter().map(|r| (r.id, 0.0)).collect();
        let mut sorted = rrf_fuse(&[vector_tuples, text_tuples], self.config().retrieval.rrf_k);

        tracing::debug!("recall: {} candidates after RRF fusion", sorted.len());

        // 5. Graph traversal: follow entity edges from top results
        //    Load graph once, find related memories via shared entities
        let mut all_scores: Vec<(Uuid, f32)> = sorted.clone();
        if let Some(ref gs) = self.graph_store {
            if let Ok(graph) = gs.load_graph(tenant_id) {
                use petgraph::visit::EdgeRef;
                use std::collections::HashSet;

                let mut visited_entities: HashSet<Uuid> = HashSet::new();
                let hop_budget = self.config().retrieval.graph_hop_limit;

                // For each top result, follow Entity edges to find related memories
                for (memory_id, score) in sorted.iter().take(budget) {
                    let memory_str = memory_id.to_string();
                    for node_idx in graph.node_indices() {
                        let node = &graph[node_idx];
                        if node.id().to_string() == memory_str {
                            // Follow outgoing edges from this memory
                            for edge_ref in graph.edges(node_idx) {
                                let edge = edge_ref.weight();
                                let target_node = &graph[edge_ref.target()];

                                // Only follow Entity edges to Entity/Concept nodes
                                if matches!(
                                    edge.edge_type,
                                    crate::types::graph::EdgeType::Entity
                                        | crate::types::graph::EdgeType::Semantic
                                ) && matches!(
                                    target_node,
                                    crate::types::graph::GraphNode::Entity { .. }
                                        | crate::types::graph::GraphNode::Concept { .. }
                                ) {
                                    let entity_id = target_node.id();
                                    if visited_entities.contains(&entity_id) {
                                        continue;
                                    }
                                    visited_entities.insert(entity_id);

                                    // Find other memories connected to this entity
                                    for reverse_edge in graph.edges(edge_ref.target()) {
                                        let connected_id = graph[reverse_edge.target()].id();
                                        let connected_str = connected_id.to_string();
                                        // Skip if already in results
                                        if all_scores.iter().any(|(id, _)| id.to_string() == connected_str) {
                                            continue;
                                        }
                                        // Add with decayed score
                                        let decayed_score = score * 0.5 * edge.weight;
                                        if hop_budget > 0 {
                                            all_scores.push((connected_id, decayed_score));
                                        }
                                    }
                                }
                            }
                            break;
                        }
                    }
                }

                // Re-sort and truncate to budget
                all_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                tracing::debug!("recall: {} candidates after graph traversal", all_scores.len());
                all_scores.truncate(budget);
            }
        }

        sorted.truncate(budget);

        // 6. Load full memories from payloads
        let mut memories = Vec::new();
        let mut result_scores = Vec::new();

        for (id, score) in all_scores {
            if let Ok(search_results) = self
                .vector_store
                .lock()
                .map_err(|e| PerspectiveError::Qdrant(e.to_string()))?
                .search(
                    tenant_id,
                    vec![0.0; self.embedder.dimensions()],
                    100,
                    self.embedder.dimensions(),
                )
            {
                if let Some(sr) = search_results.iter().find(|r| r.id == id) {
                    if let Some(payload) = &sr.payload {
                        let content = payload
                            .get("content")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let tags: Vec<String> = payload
                            .get("tags")
                            .and_then(|v| serde_json::from_value(v.clone()).ok())
                            .unwrap_or_default();
                        let metadata: HashMap<String, serde_json::Value> = payload
                            .get("metadata")
                            .and_then(|v| serde_json::from_value(v.clone()).ok())
                            .unwrap_or_default();
                        let created_at = payload
                            .get("created_at")
                            .and_then(|v| v.as_str())
                            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(Utc::now);
                        let memory_type_str = payload
                            .get("memory_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("episodic");

                        let base = MemoryBase {
                            id,
                            tenant_id: tenant_id.to_string(),
                            content,
                            embedding: None,
                            tags,
                            metadata,
                            created_at,
                            updated_at: created_at,
                        };

                        let memory = match memory_type_str {
                            "semantic" => Memory::Semantic(SemanticMemory {
                                base,
                                confidence: 0.8,
                                source_ids: vec![],
                                access_count: 0,
                                last_accessed: Utc::now(),
                                stability: 10.0,
                                first_seen: Utc::now(),
                                last_validated: None,
                            }),
                            "procedural" => Memory::Procedural(ProceduralMemory {
                                base,
                                code: None,
                                preconditions: vec![],
                                postconditions: vec![],
                                success_rate: 1.0,
                                access_count: 0,
                                last_used: Utc::now(),
                                stability: f32::INFINITY,
                                version: 1,
                            }),
                            _ => Memory::Episodic(EpisodicMemory {
                                base,
                                timestamp: Utc::now(),
                                context: None,
                                importance: 0.5,
                                access_count: 0,
                                last_accessed: Utc::now(),
                                stability: 1.0,
                                source_session: None,
                            }),
                        };

                        memories.push(memory);
                    }
                }
            }
            result_scores.push(score);
        }

        // Apply Ebbinghaus decay: filter weak memories and reinforce accessed ones
        if self.config().decay.enabled {
            memories.retain(|m| memory_strength(m) >= self.config().decay.retrieval_threshold);
            for memory in &mut memories {
                match memory {
                    Memory::Episodic(ref mut e) => {
                        e.access_count += 1;
                        e.last_accessed = Utc::now();
                        e.stability = reinforce(
                            e.stability,
                            self.config().decay.learning_rate,
                            e.access_count,
                        );
                    }
                    Memory::Semantic(ref mut s) => {
                        s.access_count += 1;
                        s.last_accessed = Utc::now();
                        s.stability = reinforce(
                            s.stability,
                            self.config().decay.learning_rate,
                            s.access_count,
                        );
                    }
                    Memory::Procedural(ref mut p) => {
                        p.access_count += 1;
                        p.last_used = Utc::now();
                        // Procedural stability is effectively infinite — no reinforcement
                    }
                }
            }
        }

        tracing::info!(
            "recall: complete {} results in {:?}",
            memories.len(),
            recall_start.elapsed(),
        );

        self.monitor.record_event(
            "recall",
            None,
            Some(query),
            true,
            Some(&serde_json::json!({
                "query": query,
                "result_count": memories.len(),
                "budget": budget,
                "results": memories.iter().map(|m| {
                    serde_json::json!({
                        "id": m.id().to_string(),
                        "content": m.content(),
                        "type": m.memory_type().to_string(),
                    })
                }).collect::<Vec<_>>()
            }).to_string()),
        );

        Ok(RecallResult {
            memories,
            scores: result_scores,
        })
    }

    /// Get a specific memory by ID.
    pub async fn get_memory(&self, tenant_id: &str, id: Uuid) -> Result<Memory> {
        let results = self
            .vector_store
            .lock()
            .map_err(|e| PerspectiveError::Qdrant(e.to_string()))?
            .search(
                tenant_id,
                vec![0.0; self.embedder.dimensions()],
                100,
                self.embedder.dimensions(),
            )?;

        for sr in results {
            if sr.id == id {
                if let Some(payload) = &sr.payload {
                    let content = payload
                        .get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    return Ok(Memory::Episodic(EpisodicMemory {
                        base: MemoryBase {
                            id,
                            tenant_id: tenant_id.to_string(),
                            content,
                            embedding: None,
                            tags: vec![],
                            metadata: Default::default(),
                            created_at: Utc::now(),
                            updated_at: Utc::now(),
                        },
                        timestamp: Utc::now(),
                        context: None,
                        importance: 0.5,
                        access_count: 0,
                        last_accessed: Utc::now(),
                        stability: 1.0,
                        source_session: None,
                    }));
                }
            }
        }

        Err(PerspectiveError::MemoryNotFound(id.to_string()))
    }

    /// Delete a memory from all stores.
    pub async fn delete_memory(&self, tenant_id: &str, id: Uuid) -> Result<()> {
        self.vector_store
            .lock()
            .map_err(|e| PerspectiveError::Qdrant(e.to_string()))?
            .delete(tenant_id, id, self.embedder.dimensions())?;
        self.text_store.delete_document(tenant_id, id)?;
        self.monitor
            .record_event("delete", None, Some(&id.to_string()), true, None);
        Ok(())
    }

    /// List tenants.
    pub async fn list_tenants(&self) -> Result<Vec<String>> {
        Ok(vec![])
    }

    // ── Dashboard query methods ──────────────────────────────────────────

    /// Build a full StatusResponse from live engine data.
    pub fn status_response(&self) -> StatusResponse {
        let monitor = &self.monitor;
        let uptime = monitor.uptime_secs();
        let decay = monitor.decay_status();
        let gc_candidates = decay.gc_candidates;
        let extraction_queue = monitor.extraction_queue().len();

        // Count from Tantivy (supports concurrent reads, no WAL lock)
        let total_memories = self.text_store.count();

        // Count memory types from vector store payloads
        let memory_types = if let Ok(mut vs) = self.vector_store.lock() {
            let zero_vec = vec![0.0; self.embedder.dimensions()];
            if let Ok(results) = vs.search(
                "default",  // count across all types for default tenant
                zero_vec,
                total_memories as usize,
                self.embedder.dimensions(),
            ) {
                let mut episodic = 0u64;
                let mut semantic = 0u64;
                let mut procedural = 0u64;
                for r in &results {
                    if let Some(payload) = &r.payload {
                        match payload.get("memory_type").and_then(|v| v.as_str()) {
                            Some("semantic") => semantic += 1,
                            Some("procedural") => procedural += 1,
                            _ => episodic += 1,
                        }
                    } else {
                        episodic += 1;
                    }
                }
                MemoryTypeCounts { episodic, semantic, procedural }
            } else {
                MemoryTypeCounts::default()
            }
        } else {
            MemoryTypeCounts::default()
        };

        let graph = self.get_graph_stats().graph;

        StatusResponse {
            health: "healthy".to_string(),
            uptime_secs: uptime,
            total_memories,
            memory_types,
            gc_candidates,
            extraction_queue,
            graph,
        }
    }

    /// Get recent activity events.
    pub fn get_activity(&self, limit: usize) -> ActivityResponse {
        ActivityResponse {
            events: self.monitor.get_events(limit),
        }
    }

    /// Get process status (consolidation, decay, extraction).
    pub fn get_processes(&self) -> ProcessesResponse {
        ProcessesResponse {
            consolidation: self.monitor.consolidation_status(),
            decay: self.monitor.decay_status(),
            extraction_queue: self.monitor.extraction_queue(),
            consolidation_history: self.monitor.consolidation_history(),
        }
    }

    /// Get graph stats.
    pub fn get_graph_stats(&self) -> GraphResponse {
        let (total_nodes, total_edges, nodes, edges) = match &self.graph_store {
            Some(gs) => match gs.count_all() {
                Ok(result) => result,
                Err(e) => {
                    eprintln!("[perspective] graph count_all error: {e}");
                    (0, 0, vec![], vec![])
                }
            },
            None => {
                eprintln!("[perspective] graph_store is None");
                (0, 0, vec![], vec![])
            }
        };

        // Count node types
        let mut memory_ref = 0u64;
        let mut entity = 0u64;
        let mut concept = 0u64;
        for node in &nodes {
            match node {
                GraphNode::MemoryRef { .. } => memory_ref += 1,
                GraphNode::Entity { .. } => entity += 1,
                GraphNode::Concept { .. } => concept += 1,
            }
        }

        // Count edge types
        let mut edge_types: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();
        for edge in &edges {
            let name = format!("{:?}", edge.edge_type);
            *edge_types.entry(name).or_insert(0) += 1;
        }

        // Recent edges (last 10)
        let mut sorted_edges = edges.clone();
        sorted_edges.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        let recent_edges: Vec<RecentEdge> = sorted_edges
            .into_iter()
            .take(10)
            .map(|e| RecentEdge {
                created_at: Some(e.created_at),
                edge_type: format!("{:?}", e.edge_type),
                from_id: e.from_id.to_string(),
                to_id: e.to_id.to_string(),
                weight: e.weight,
            })
            .collect();

        // Average connectivity (edges per node)
        let avg_connectivity = if total_nodes > 0 {
            total_edges as f32 / total_nodes as f32
        } else {
            0.0
        };

        GraphResponse {
            graph: GraphStats {
                total_nodes,
                total_edges,
                communities: 0, // TODO: run community detection
                avg_connectivity,
                node_types: GraphNodeTypeCounts {
                    memory_ref,
                    entity,
                    concept,
                },
                edge_types,
                recent_edges,
            },
        }
    }

    /// Get full graph data for interactive visualization (react-force-graph).
    pub fn get_full_graph(&self) -> serde_json::Value {
        let (nodes, edges) = match &self.graph_store {
            Some(gs) => match gs.count_all() {
                Ok((_, _, nodes, edges)) => (nodes, edges),
                Err(_) => (vec![], vec![]),
            }
            None => (vec![], vec![]),
        };

        let viz_nodes: Vec<serde_json::Value> = nodes
            .iter()
            .map(|n| {
                let (id, label, node_type, group) = match n {
                    super::types::graph::GraphNode::MemoryRef { id, memory_type } => {
                        (id.to_string(), format!("{:?}", memory_type), "memory", 1)
                    }
                    super::types::graph::GraphNode::Entity { id, name, entity_type } => {
                        (id.to_string(), name.clone(), "entity", 2)
                    }
                    super::types::graph::GraphNode::Concept { id, label } => {
                        (id.to_string(), label.clone(), "concept", 3)
                    }
                };
                serde_json::json!({
                    "id": id,
                    "label": label,
                    "type": node_type,
                    "group": group,
                })
            })
            .collect();

        let viz_links: Vec<serde_json::Value> = edges
            .iter()
            .map(|e| {
                serde_json::json!({
                    "source": e.from_id.to_string(),
                    "target": e.to_id.to_string(),
                    "type": format!("{:?}", e.edge_type),
                    "weight": e.weight,
                })
            })
            .collect();

        serde_json::json!({
            "nodes": viz_nodes,
            "links": viz_links,
        })
    }

    /// Get engine config for dashboard display.
    pub fn get_config_response(&self) -> ConfigResponse {
        let mut storage = HashMap::new();
        storage.insert(
            "data_dir".into(),
            self.config().storage.data_dir.display().to_string(),
        );
        storage.insert(
            "embedded_qdrant".into(),
            self.config().storage.embedded_qdrant.to_string(),
        );

        let mut embedding = HashMap::new();
        match &self.config().embedding {
            crate::config::EmbeddingConfig::Local { model } => {
                embedding.insert("type".into(), "local".into());
                embedding.insert("model".into(), model.clone());
            }
            crate::config::EmbeddingConfig::Api {
                endpoint, model, ..
            } => {
                embedding.insert("type".into(), "api".into());
                embedding.insert("endpoint".into(), endpoint.clone());
                embedding.insert("model".into(), model.clone());
            }
        }

        let mut decay = HashMap::new();
        decay.insert(
            "enabled".into(),
            self.config().decay.enabled.to_string(),
        );
        decay.insert(
            "episodic_lambda".into(),
            self.config().decay.episodic_lambda.to_string(),
        );
        decay.insert(
            "semantic_lambda".into(),
            self.config().decay.semantic_lambda.to_string(),
        );
        decay.insert(
            "procedural_lambda".into(),
            self.config().decay.procedural_lambda.to_string(),
        );
        decay.insert(
            "learning_rate".into(),
            self.config().decay.learning_rate.to_string(),
        );
        decay.insert(
            "retrieval_threshold".into(),
            self.config().decay.retrieval_threshold.to_string(),
        );
        decay.insert(
            "gc_threshold".into(),
            self.config().decay.gc_threshold.to_string(),
        );

        let mut retrieval = HashMap::new();
        retrieval.insert(
            "default_budget".into(),
            self.config().retrieval.default_budget.to_string(),
        );
        retrieval.insert(
            "vector_overfetch".into(),
            self.config().retrieval.vector_overfetch.to_string(),
        );
        retrieval.insert(
            "graph_hop_limit".into(),
            self.config().retrieval.graph_hop_limit.to_string(),
        );
        retrieval.insert("rrf_k".into(), self.config().retrieval.rrf_k.to_string());

        let mut consolidation = HashMap::new();
        consolidation.insert(
            "enabled".into(),
            self.config().consolidation.enabled.to_string(),
        );
        consolidation.insert(
            "interval_secs".into(),
            self.config().consolidation.interval_secs.to_string(),
        );
        consolidation.insert(
            "dedup_similarity_threshold".into(),
            self.config()
                .consolidation
                .dedup_similarity_threshold
                .to_string(),
        );
        consolidation.insert(
            "promotion_access_count".into(),
            self.config().consolidation.promotion_access_count.to_string(),
        );
        consolidation.insert(
            "staleness_days".into(),
            self.config().consolidation.staleness_days.to_string(),
        );

        let mut extraction = HashMap::new();
        extraction.insert("enabled".into(), self.config().extraction.enabled.to_string());
        extraction.insert("endpoint".into(), self.config().extraction.endpoint.clone());
        extraction.insert("model".into(), self.config().extraction.model.clone());
        extraction.insert(
            "batch_size".into(),
            self.config().extraction.batch_size.to_string(),
        );
        extraction.insert(
            "batch_interval_secs".into(),
            self.config().extraction.batch_interval_secs.to_string(),
        );
        extraction.insert(
            "importance_gate".into(),
            self.config().extraction.importance_gate.to_string(),
        );

        ConfigResponse {
            storage,
            embedding,
            decay,
            retrieval,
            consolidation,
            extraction,
        }
    }

    /// Update settings from dashboard. Reads TOML, applies patch, writes back,
    /// and updates in-memory config. Changes take effect immediately.
    pub fn update_settings(&self, patch: &serde_json::Value) -> Result<()> {
        // Read current config from TOML file (source of truth)
        let config_path = {
            let cfg = self.config.read().unwrap();
            cfg.storage.data_dir.join("perspective.toml")
        };

        let mut current = if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)
                .map_err(|e| PerspectiveError::Config(format!("read config: {e}")))?;
            toml::from_str::<Config>(&content)
                .map_err(|e| PerspectiveError::Config(format!("parse config: {e}")))?
        } else {
            self.config.read().unwrap().clone()
        };

        // Apply patch
        if let Some(decay) = patch.get("decay") {
            if let Some(v) = decay.get("enabled").and_then(|v| v.as_bool()) {
                current.decay.enabled = v;
            }
            if let Some(v) = decay.get("episodic_lambda").and_then(|v| v.as_f64()) {
                current.decay.episodic_lambda = v as f32;
            }
            if let Some(v) = decay.get("semantic_lambda").and_then(|v| v.as_f64()) {
                current.decay.semantic_lambda = v as f32;
            }
            if let Some(v) = decay.get("procedural_lambda").and_then(|v| v.as_f64()) {
                current.decay.procedural_lambda = v as f32;
            }
            if let Some(v) = decay.get("learning_rate").and_then(|v| v.as_f64()) {
                current.decay.learning_rate = v as f32;
            }
            if let Some(v) = decay.get("retrieval_threshold").and_then(|v| v.as_f64()) {
                current.decay.retrieval_threshold = v as f32;
            }
            if let Some(v) = decay.get("gc_threshold").and_then(|v| v.as_f64()) {
                current.decay.gc_threshold = v as f32;
            }
        }
        if let Some(extraction) = patch.get("extraction") {
            if let Some(v) = extraction.get("enabled").and_then(|v| v.as_bool()) {
                current.extraction.enabled = v;
            }
        }
        if let Some(consolidation) = patch.get("consolidation") {
            if let Some(v) = consolidation.get("enabled").and_then(|v| v.as_bool()) {
                current.consolidation.enabled = v;
            }
        }

        // Write to file
        let toml_str = toml::to_string_pretty(&current)
            .map_err(|e| PerspectiveError::Config(format!("serialize config: {e}")))?;
        std::fs::write(&config_path, &toml_str)
            .map_err(|e| PerspectiveError::Config(format!("write config: {e}")))?;

        // Update in-memory config
        {
            let mut cfg = self.config.write().unwrap();
            *cfg = current;
        }

        tracing::info!("settings: config updated at {}", config_path.display());
        Ok(())
    }

    /// List memories from the vector store for a tenant.
    /// Used by the dashboard /api/memories endpoint.
    /// Searches with a zero vector to retrieve all stored memories.
    pub fn list_memories(&self, tenant_id: &str, query: &str, limit: usize) -> MemoriesResponse {
        // Use Tantivy for read-only listing (supports concurrent readers)
        let results = if query.is_empty() {
            self.text_store.list_all(limit).unwrap_or_default()
        } else {
            self.text_store
                .search(tenant_id, query, limit)
                .unwrap_or_default()
        };

        let total = self.text_store.count();

        let memories: Vec<MemorySummary> = results
            .into_iter()
            .map(|sr| MemorySummary {
                id: sr.id.to_string(),
                memory_type: sr.memory_type,
                content: sr.content,
                tags: vec![],
                created_at: Utc::now(),
                updated_at: Utc::now(),
                importance: None,
                stability: None,
                access_count: 0,
                last_accessed: Utc::now(),
                source_session: None,
            })
            .collect();

        MemoriesResponse { memories, total }
    }

    /// Process a batch of buffered texts through the LLM extraction pipeline.
    /// Call this periodically (e.g., from a background thread or the extraction loop).
    /// Returns the number of facts extracted.
    pub async fn process_extraction_batch(&self) -> Result<usize> {
        let pipeline = match &self.extraction_pipeline {
            Some(p) => p,
            None => return Ok(0),
        };

        let (items, token_count) = {
            let mut batcher = self
                .batcher
                .lock()
                .map_err(|e| PerspectiveError::Config(e.to_string()))?;
            if !batcher.should_flush() {
                return Ok(0);
            }
            let tokens = batcher.current_tokens();
            (batcher.drain(), tokens)
        };

        if items.is_empty() {
            return Ok(0);
        }

        tracing::info!(
            "extraction: processing batch ({} items, ~{} tokens)",
            items.len(),
            token_count
        );

        // Extract text for the pipeline, keep tenant_ids for storing results
        let texts: Vec<&str> = items.iter().map(|(_, t)| t.as_str()).collect();
        let facts = pipeline.extract_batch(&texts).await?;

        // Store extracted facts under the same tenant as the source document
        let fact_count = facts.len();
        for (item, fact) in items.iter().zip(&facts) {
            let store_req = StoreRequest {
                tenant_id: item.0.clone(),
                content: fact.fact.clone(),
                memory_type: MemoryType::Semantic,
                tags: vec!["extracted".to_string()],
                metadata: {
                    let mut m = std::collections::HashMap::new();
                    m.insert(
                        "source".to_string(),
                        serde_json::Value::String("extraction".to_string()),
                    );
                    m.insert(
                        "entities".to_string(),
                        serde_json::Value::Array(
                            fact.entities
                                .iter()
                                .map(|e| serde_json::Value::String(e.clone()))
                                .collect(),
                        ),
                    );
                    m
                },
                context: Some(fact.source_text.clone()),
                source_session: None,
                skip_extraction: true,
            };

            let _ = self.store(store_req).await;
        }

        self.monitor.record_event(
            "extraction",
            Some("llm"),
            Some(&format!("{} facts extracted", fact_count)),
            true,
            Some(&serde_json::json!({"fact_count": fact_count}).to_string()),
        );

        Ok(fact_count)
    }

    /// Get the number of memories queued for LLM extraction.
    pub fn extraction_queue_len(&self) -> usize {
        self.batcher.lock().map(|b| b.len()).unwrap_or(0)
    }

    /// Start the background extraction loop.
    /// Runs every `batch_interval_secs` and processes buffered texts through the LLM.
    /// Returns a JoinHandle that can be used to stop the loop.
    pub fn start_extraction_loop(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let interval = std::time::Duration::from_secs(self.config().extraction.batch_interval_secs);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                match self.process_extraction_batch().await {
                    Ok(n) if n > 0 => {
                        tracing::info!("Extraction loop: processed {} facts", n);
                    }
                    Ok(_) => {} // nothing to process
                    Err(e) => {
                        tracing::warn!("Extraction loop error: {}", e);
                    }
                }

                // Check if consolidation was flagged by store()
                if self.pending_consolidation.swap(false, std::sync::atomic::Ordering::Relaxed) {
                    tracing::info!("consolidation: running (triggered by 100 stores)");
                    if let Ok(tenants) = self.list_tenants().await {
                        for tenant_id in &tenants {
                            match self.run_consolidation(tenant_id).await {
                                Ok(report) => {
                                    tracing::info!(
                                        "consolidation: {} done — duplicates={}, promotable={}, communities={}",
                                        tenant_id,
                                        report.duplicates_found,
                                        report.promotable_count,
                                        report.communities
                                    );
                                }
                                Err(e) => tracing::warn!("consolidation: {} failed: {e}", tenant_id),
                            }
                        }
                    }
                }
            }
        })
    }

    /// Run a single decay tick across all tenants.
    /// Computes Ebbinghaus forgetting curve for each memory, garbage-collects
    /// stale memories, and logs stats. Called once daily at midnight.
    pub async fn run_decay_tick(&self) {
        if !self.config().decay.enabled {
            tracing::debug!("decay: disabled, skipping");
            return;
        }
        tracing::info!("decay: starting daily tick");
        let mut total_processed = 0u64;
        let mut total_gc = 0u64;
        match self.list_tenants().await {
            Ok(tenants) => {
                tracing::debug!("decay: processing {} tenants", tenants.len());
                for tenant_id in &tenants {
                    let dims = self.embedder.dimensions();
                    let memories = match self.vector_store.lock() {
                        Ok(mut vs) => match vs.search(
                            tenant_id,
                            vec![0.0; dims],
                            10_000,
                            dims,
                        ) {
                            Ok(results) => {
                                tracing::debug!("decay: vector search returned {} results for {tenant_id}", results.len());
                                let mut mems = Vec::new();
                                for sr in &results {
                                    if let Some(payload) = &sr.payload {
                                        let content = payload
                                            .get("content")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string();
                                        let memory_type_str = payload
                                            .get("memory_type")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("episodic");
                                        let mem = match memory_type_str {
                                            "semantic" => Memory::Semantic(SemanticMemory {
                                                base: MemoryBase {
                                                    id: sr.id,
                                                    tenant_id: tenant_id.to_string(),
                                                    content,
                                                    embedding: None,
                                                    tags: vec![],
                                                    metadata: Default::default(),
                                                    created_at: Utc::now(),
                                                    updated_at: Utc::now(),
                                                },
                                                confidence: 0.8,
                                                source_ids: vec![],
                                                access_count: 0,
                                                last_accessed: Utc::now(),
                                                stability: 10.0,
                                                first_seen: Utc::now(),
                                                last_validated: None,
                                            }),
                                            "procedural" => Memory::Procedural(ProceduralMemory {
                                                base: MemoryBase {
                                                    id: sr.id,
                                                    tenant_id: tenant_id.to_string(),
                                                    content,
                                                    embedding: None,
                                                    tags: vec![],
                                                    metadata: Default::default(),
                                                    created_at: Utc::now(),
                                                    updated_at: Utc::now(),
                                                },
                                                code: None,
                                                preconditions: vec![],
                                                postconditions: vec![],
                                                success_rate: 1.0,
                                                access_count: 0,
                                                last_used: Utc::now(),
                                                stability: f32::INFINITY,
                                                version: 1,
                                            }),
                                            _ => Memory::Episodic(EpisodicMemory {
                                                base: MemoryBase {
                                                    id: sr.id,
                                                    tenant_id: tenant_id.to_string(),
                                                    content,
                                                    embedding: None,
                                                    tags: vec![],
                                                    metadata: Default::default(),
                                                    created_at: Utc::now(),
                                                    updated_at: Utc::now(),
                                                },
                                                timestamp: Utc::now(),
                                                context: None,
                                                importance: 0.5,
                                                access_count: 0,
                                                last_accessed: Utc::now(),
                                                stability: 1.0,
                                                source_session: None,
                                            }),
                                        };
                                        mems.push(mem);
                                    }
                                }
                                Ok(mems)
                            }
                            Err(e) => {
                                tracing::warn!("decay: vector search failed for {tenant_id}: {e}");
                                Err(())
                            }
                        },
                        Err(e) => {
                            tracing::warn!("decay: mutex poisoned for {tenant_id}: {e}");
                            Err(())
                        },
                    };
                    if let Ok(mems) = memories {
                        let count = mems.len() as u64;
                        tracing::debug!("decay: fetched {} memories for {tenant_id}", count);
                        let _results = crate::decay::maintenance::apply_decay_to_memories(
                            &mems,
                            &self.config().decay,
                        );
                        // Garbage-collect memories below threshold
                        let gc_candidates = crate::decay::maintenance::get_gc_candidates(
                            &mems,
                            &self.config().decay,
                        );
                        let gc_count = gc_candidates.len() as u64;
                        for mem in gc_candidates {
                            let id = mem.id();
                            if let Err(e) = self.delete_memory(tenant_id, id).await {
                                tracing::warn!("decay: failed to delete GC candidate {id}: {e}");
                            }
                        }
                        tracing::info!(
                            "decay: tenant {tenant_id} — processed {}, garbage-collected {}",
                            count,
                            gc_count
                        );
                        total_processed += count;
                        total_gc += gc_count;
                    }
                }
            }
            Err(e) => tracing::warn!("decay: failed to list tenants: {e}"),
        }
        tracing::info!(
            "decay: tick complete — processed {total_processed} memories, garbage-collected {total_gc}"
        );
        self.monitor.record_event(
            "decay",
            None,
            None,
            true,
            Some(&serde_json::json!({
                "total_processed": total_processed,
                "total_gc": total_gc,
            }).to_string()),
        );
    }

    /// Load all memories for a tenant with embeddings (for consolidation).
    /// Uses the vector store to retrieve vectors alongside payloads.
    fn load_all_memories(&self, tenant_id: &str, limit: usize) -> Result<Vec<Memory>> {
        let zero_vec = vec![0.0; self.embedder.dimensions()];
        let results = self
            .vector_store
            .lock()
            .map_err(|e| PerspectiveError::Qdrant(e.to_string()))?
            .search_with_vectors(tenant_id, zero_vec, limit, self.embedder.dimensions())?;

        let mut memories = Vec::new();
        for sr in results {
            let content = sr
                .payload
                .as_ref()
                .and_then(|p| p.get("content"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let tags: Vec<String> = sr
                .payload
                .as_ref()
                .and_then(|p| p.get("tags"))
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let metadata: HashMap<String, serde_json::Value> = sr
                .payload
                .as_ref()
                .and_then(|p| p.get("metadata"))
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let created_at = sr
                .payload
                .as_ref()
                .and_then(|p| p.get("created_at"))
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now);
            let memory_type_str = sr
                .payload
                .as_ref()
                .and_then(|p| p.get("memory_type"))
                .and_then(|v| v.as_str())
                .unwrap_or("episodic");
            let access_count = sr
                .payload
                .as_ref()
                .and_then(|p| p.get("access_count"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            let base = MemoryBase {
                id: sr.id,
                tenant_id: tenant_id.to_string(),
                content,
                embedding: sr.vector,
                tags,
                metadata,
                created_at,
                updated_at: created_at,
            };

            let memory = match memory_type_str {
                "semantic" => Memory::Semantic(SemanticMemory {
                    base,
                    confidence: 0.8,
                    source_ids: vec![],
                    access_count,
                    last_accessed: Utc::now(),
                    stability: 10.0,
                    first_seen: Utc::now(),
                    last_validated: None,
                }),
                "procedural" => Memory::Procedural(ProceduralMemory {
                    base,
                    code: None,
                    preconditions: vec![],
                    postconditions: vec![],
                    success_rate: 1.0,
                    access_count,
                    last_used: Utc::now(),
                    stability: f32::INFINITY,
                    version: 1,
                }),
                _ => Memory::Episodic(EpisodicMemory {
                    base,
                    timestamp: Utc::now(),
                    context: None,
                    importance: 0.5,
                    access_count,
                    last_accessed: Utc::now(),
                    stability: 1.0,
                    source_session: None,
                }),
            };
            memories.push(memory);
        }
        Ok(memories)
    }

    /// Run a single consolidation pass: dedup, promotion, and community detection.
    pub async fn run_consolidation(&self, tenant_id: &str) -> Result<ConsolidationReport> {
        tracing::info!("consolidation: starting for tenant={tenant_id}");
        let consol_start = std::time::Instant::now();
        let memories = self.load_all_memories(tenant_id, 10000)?;
        tracing::debug!("consolidation: loaded {} memories", memories.len());
        let mut report = ConsolidationReport::default();

        if self.config().consolidation.enabled {
            let threshold = self.config().consolidation.dedup_similarity_threshold;
            let duplicates = find_duplicates(&memories, threshold);
            report.duplicates_found = duplicates.len();
            tracing::debug!("consolidation: found {} duplicates", duplicates.len());

            let promotable =
                find_promotable(&memories, self.config().consolidation.promotion_access_count);
            report.promotable_count = promotable.len();
            tracing::debug!("consolidation: {} promotable memories", promotable.len());
        }

        if let Some(ref gs) = self.graph_store {
            if let Ok(graph) = gs.load_graph(tenant_id) {
                let communities = detect_communities(&graph);
                report.communities = communities.len();
            }
        }

        tracing::info!(
            "consolidation: complete for {tenant_id} ({}) dupes, ({}) promotable, ({}) communities in {:?}",
            report.duplicates_found,
            report.promotable_count,
            report.communities,
            consol_start.elapsed(),
        );
        Ok(report)
    }

    /// Start the background consolidation loop.
    /// Runs every `interval_secs` and processes all tenants.
    pub fn start_consolidation_loop(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let interval = std::time::Duration::from_secs(self.config().consolidation.interval_secs);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                if !self.config().consolidation.enabled {
                    continue;
                }
                match self.list_tenants().await {
                    Ok(tenants) => {
                        for tenant_id in &tenants {
                            match self.run_consolidation(tenant_id).await {
                                Ok(report) => {
                                    tracing::info!(
                                        "consolidation: {} duplicates, {} promotable, {} communities in {}",
                                        tenant_id,
                                        report.duplicates_found,
                                        report.promotable_count,
                                        report.communities
                                    );
                                }
                                Err(e) => {
                                    tracing::error!("consolidation: failed for {tenant_id}: {e}");
                                }
                            }
                        }
                    }
                    Err(e) => tracing::warn!("consolidation: failed to list tenants: {e}"),
                }
            }
        })
    }
}
