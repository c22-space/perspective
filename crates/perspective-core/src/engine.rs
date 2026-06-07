use crate::config::Config;
use crate::embedding::Embedder;
use crate::embedding::LocalEmbedder;
use crate::error::{PerspectiveError, Result};
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
    graph_store: Arc<GraphStore>,
    text_store: Arc<TextStore>,
    embedder: Arc<dyn Embedder>,
    pub monitor: Arc<Monitor>,
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

        Ok(Self {
            config,
            vector_store,
            graph_store,
            text_store,
            embedder,
            monitor,
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

        // Store in graph
        self.graph_store.save_node(
            &req.tenant_id,
            &GraphNode::MemoryRef {
                id,
                memory_type: req.memory_type,
            },
        )?;

        // Store in text index
        self.text_store
            .add_document(&req.tenant_id, id, &req.content)?;

        // Create temporal edge to most recent memory of same type
        if let Ok(neighbors) = self.graph_store.get_neighbors(&req.tenant_id, id, None) {
            if let Some((last_node, _)) = neighbors.last() {
                let edge = GraphEdge {
                    from_id: id,
                    to_id: last_node.id(),
                    edge_type: EdgeType::Temporal,
                    weight: 0.8,
                    created_at: now,
                    last_reinforced: now,
                    decay_rate: self.config.decay.episodic_lambda,
                };
                self.graph_store.save_edge(&req.tenant_id, &edge)?;
            }
        }

        self.monitor.record_event(
            "store",
            Some(&req.memory_type.to_string()),
            Some(&req.content),
            true,
        );

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
        let event_count = monitor.event_count();
        let uptime = monitor.uptime_secs();
        let _consolidation = monitor.consolidation_status();
        let decay = monitor.decay_status();
        let gc_candidates = decay.gc_candidates;
        let extraction_queue = monitor.extraction_queue().len();

        StatusResponse {
            health: "healthy".to_string(),
            uptime_secs: uptime,
            total_memories: event_count,
            memory_types: MemoryTypeCounts::default(),
            gc_candidates,
            extraction_queue,
            graph: GraphStats::default(),
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
        GraphResponse {
            graph: GraphStats::default(),
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

    /// List memories from the text index for a tenant.
    /// Used by the dashboard /api/memories endpoint.
    pub fn list_memories(
        &self,
        tenant_id: &str,
        query: &str,
        limit: usize,
    ) -> MemoriesResponse {
        let text_results = if query.trim().is_empty() {
            // Empty query: can't use Tantivy wildcard, return empty
            vec![]
        } else {
            match self.text_store.search(tenant_id, query, limit) {
                Ok(r) => r,
                Err(_) => vec![],
            }
        };

        let mut memories = Vec::new();
        for tr in text_results {
            // Load full memory from vector store payload
            if let Ok(mut vs) = self.vector_store.lock().map_err(|e| PerspectiveError::Qdrant(e.to_string())) {
                let results = vs
                    .search(
                        tenant_id,
                        vec![0.0; self.embedder.dimensions()],
                        200,
                        self.embedder.dimensions(),
                    )
                    .unwrap_or_default();

                if let Some(sr) = results.iter().find(|s| s.id == tr.id) {
                    let content = sr
                        .payload
                        .as_ref()
                        .and_then(|p| p.get("content"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let mem_type = sr
                        .payload
                        .as_ref()
                        .and_then(|p| p.get("memory_type"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("episodic")
                        .to_string();
                    let tags: Vec<String> = sr
                        .payload
                        .as_ref()
                        .and_then(|p| p.get("tags"))
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                        .unwrap_or_default();

                    memories.push(MemorySummary {
                        id: tr.id.to_string(),
                        memory_type: mem_type,
                        content,
                        tags,
                        created_at: Utc::now(),
                        updated_at: Utc::now(),
                        importance: None,
                        stability: None,
                        access_count: 0,
                        last_accessed: Utc::now(),
                        source_session: None,
                    });
                }
            }
        }

        MemoriesResponse { memories }
    }
}
