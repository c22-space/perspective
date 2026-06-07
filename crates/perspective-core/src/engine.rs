use crate::config::Config;
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
use chrono::Utc;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub struct PerspectiveEngine {
    pub config: Config,
    vector_store: Mutex<QdrantVectorStore>,
    graph_store: Option<Arc<GraphStore>>,
    text_store: Arc<TextStore>,
    embedder: Arc<dyn Embedder>,
    pub monitor: Arc<Monitor>,
    extraction_pipeline: Option<Arc<ExtractionPipeline>>,
    batcher: Mutex<ExtractionBatcher>,
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
            config,
            vector_store,
            graph_store: Some(graph_store),
            text_store,
            embedder,
            monitor,
            extraction_pipeline,
            batcher: Mutex::new(batcher),
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
            config,
            vector_store,
            graph_store,
            text_store,
            embedder,
            monitor,
            extraction_pipeline,
            batcher: Mutex::new(batcher),
        })
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn embedder(&self) -> &dyn Embedder {
        self.embedder.as_ref()
    }

    /// Store a memory in all three stores.
    pub async fn store(&self, req: StoreRequest) -> Result<Uuid> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        // Generate embedding (blocking call for local model)
        let embedding = self
            .embedder
            .embed(&[&req.content])
            .await
            .map_err(|e| PerspectiveError::Embedding(e.to_string()))?
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
                        decay_rate: self.config.decay.episodic_lambda,
                    };
                    gs.save_edge(&req.tenant_id, &edge)?;
                }
            }
        }

        // Store in text index
        self.text_store
            .add_document(&req.tenant_id, id, &req.content)?;

        self.monitor.record_event(
            "store",
            Some(&req.memory_type.to_string()),
            Some(&req.content),
            true,
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

                // 2. Extract relations locally
                let relations = extract_relations(&content, &entities);

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
                );
            }

            // 5. Buffer for LLM extraction (processed by start_extraction_loop)
            if self.config.extraction.enabled && self.extraction_pipeline.is_some() {
                let pipeline = self.extraction_pipeline.as_ref().unwrap();
                if pipeline.is_memorable(&req.content) {
                    if let Ok(mut batcher) = self.batcher.lock() {
                        batcher.buffer(&req.content);
                    }
                }
            }
        } // end skip_extraction guard

        Ok(id)
    }

    /// Recall relevant memories for a query.
    pub async fn recall(
        &self,
        tenant_id: &str,
        query: &str,
        budget: usize,
    ) -> Result<RecallResult> {
        let _now = Utc::now();
        let overfetch = budget * self.config.retrieval.vector_overfetch;

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

        // 2. Text search (BM25)
        let text_results = self.text_store.search(tenant_id, query, overfetch)?;

        // 3. Merge and deduplicate
        let mut scores: std::collections::HashMap<Uuid, f32> = std::collections::HashMap::new();

        // Reciprocal Rank Fusion for vector results
        for (rank, result) in vector_results.iter().enumerate() {
            let rrf_score = 1.0 / (self.config.retrieval.rrf_k + rank as f32 + 1.0);
            *scores.entry(result.id).or_insert(0.0) += rrf_score;
        }

        // Reciprocal Rank Fusion for text results
        for (rank, result) in text_results.iter().enumerate() {
            let rrf_score = 1.0 / (self.config.retrieval.rrf_k + rank as f32 + 1.0);
            *scores.entry(result.id).or_insert(0.0) += rrf_score;
        }

        // 4. Sort by score
        let mut sorted: Vec<(Uuid, f32)> = scores.into_iter().collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        sorted.truncate(budget);

        // 5. Load full memories from payloads
        let mut memories = Vec::new();
        let mut result_scores = Vec::new();

        for (id, score) in sorted {
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

                        memories.push(Memory::Episodic(EpisodicMemory {
                            base: MemoryBase {
                                id,
                                tenant_id: tenant_id.to_string(),
                                content,
                                embedding: None,
                                tags,
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
            result_scores.push(score);
        }

        self.monitor.record_event("recall", None, Some(query), true);

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
            .record_event("delete", None, Some(&id.to_string()), true);
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

        let graph = self.get_graph_stats().graph;

        StatusResponse {
            health: "healthy".to_string(),
            uptime_secs: uptime,
            total_memories,
            memory_types: MemoryTypeCounts::default(),
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

    /// Get engine config for dashboard display.
    pub fn get_config_response(&self) -> ConfigResponse {
        let mut storage = HashMap::new();
        storage.insert(
            "data_dir".into(),
            self.config.storage.data_dir.display().to_string(),
        );
        storage.insert(
            "embedded_qdrant".into(),
            self.config.storage.embedded_qdrant.to_string(),
        );

        let mut embedding = HashMap::new();
        match &self.config.embedding {
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
            "episodic_lambda".into(),
            self.config.decay.episodic_lambda.to_string(),
        );
        decay.insert(
            "semantic_lambda".into(),
            self.config.decay.semantic_lambda.to_string(),
        );
        decay.insert(
            "procedural_lambda".into(),
            self.config.decay.procedural_lambda.to_string(),
        );
        decay.insert(
            "learning_rate".into(),
            self.config.decay.learning_rate.to_string(),
        );
        decay.insert(
            "retrieval_threshold".into(),
            self.config.decay.retrieval_threshold.to_string(),
        );
        decay.insert(
            "gc_threshold".into(),
            self.config.decay.gc_threshold.to_string(),
        );

        let mut retrieval = HashMap::new();
        retrieval.insert(
            "default_budget".into(),
            self.config.retrieval.default_budget.to_string(),
        );
        retrieval.insert(
            "vector_overfetch".into(),
            self.config.retrieval.vector_overfetch.to_string(),
        );
        retrieval.insert(
            "graph_hop_limit".into(),
            self.config.retrieval.graph_hop_limit.to_string(),
        );
        retrieval.insert("rrf_k".into(), self.config.retrieval.rrf_k.to_string());

        let mut consolidation = HashMap::new();
        consolidation.insert(
            "enabled".into(),
            self.config.consolidation.enabled.to_string(),
        );
        consolidation.insert(
            "interval_secs".into(),
            self.config.consolidation.interval_secs.to_string(),
        );
        consolidation.insert(
            "dedup_similarity_threshold".into(),
            self.config
                .consolidation
                .dedup_similarity_threshold
                .to_string(),
        );
        consolidation.insert(
            "promotion_access_count".into(),
            self.config.consolidation.promotion_access_count.to_string(),
        );
        consolidation.insert(
            "staleness_days".into(),
            self.config.consolidation.staleness_days.to_string(),
        );

        let mut extraction = HashMap::new();
        extraction.insert("enabled".into(), self.config.extraction.enabled.to_string());
        extraction.insert("endpoint".into(), self.config.extraction.endpoint.clone());
        extraction.insert("model".into(), self.config.extraction.model.clone());
        extraction.insert(
            "batch_size".into(),
            self.config.extraction.batch_size.to_string(),
        );
        extraction.insert(
            "batch_interval_secs".into(),
            self.config.extraction.batch_interval_secs.to_string(),
        );
        extraction.insert(
            "importance_gate".into(),
            self.config.extraction.importance_gate.to_string(),
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
                memory_type: "episodic".to_string(),
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

        let texts = {
            let mut batcher = self
                .batcher
                .lock()
                .map_err(|e| PerspectiveError::Config(e.to_string()))?;
            if !batcher.should_flush() {
                return Ok(0);
            }
            batcher.drain()
        };

        if texts.is_empty() {
            return Ok(0);
        }

        let refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        let facts = pipeline.extract_batch(&refs).await?;

        // Store extracted facts as semantic memories
        let fact_count = facts.len();
        for fact in &facts {
            if fact.confidence < 0.3 {
                continue;
            }

            // Store the extracted fact as a semantic memory
            let store_req = StoreRequest {
                tenant_id: "default".to_string(),
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
                        "confidence".to_string(),
                        serde_json::Value::Number(
                            serde_json::Number::from_f64(fact.confidence as f64)
                                .unwrap_or_else(|| serde_json::Number::from(0)),
                        ),
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

            // Store the fact as a semantic memory (skip_extraction prevents infinite loop)
            let _ = self.store(store_req).await;
        }

        self.monitor.record_event(
            "extraction",
            Some("llm"),
            Some(&format!("{} facts extracted", fact_count)),
            true,
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
        let interval = std::time::Duration::from_secs(self.config.extraction.batch_interval_secs);
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
            }
        })
    }
}
