use std::sync::Arc;
use chrono::Utc;
use uuid::Uuid;
use crate::config::Config;
use crate::error::{PerspectiveError, Result};
use crate::types::*;
use crate::store::vector::QdrantVectorStore;
use crate::store::graph::GraphStore;
use crate::store::text::TextStore;
use crate::embedding::local::LocalEmbedder;
use crate::embedding::Embedder;

pub struct PerspectiveEngine {
    pub config: Config,
    vector_store: Arc<QdrantVectorStore>,
    graph_store: Arc<GraphStore>,
    text_store: Arc<TextStore>,
    embedder: Arc<dyn Embedder>,
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
    pub async fn new(config: Config) -> Result<Self> {
        // Initialize embedder
        let embedder: Arc<dyn Embedder> = match &config.embedding {
            crate::config::EmbeddingConfig::Local { model } => {
                Arc::new(LocalEmbedder::new(model)?)
            }
            crate::config::EmbeddingConfig::Api { endpoint, model, api_key: _ } => {
                // TODO: implement API embedder
                return Err(PerspectiveError::Config(
                    format!("API embeddings not yet implemented (endpoint: {}, model: {})", endpoint, model)
                ));
            }
        };

        // Initialize vector store
        let qdrant_url = config.storage.qdrant_url.as_deref().unwrap_or("http://localhost:6334");
        let qdrant_api_key = config.storage.qdrant_api_key.as_deref();
        let vector_store = Arc::new(QdrantVectorStore::new(qdrant_url, qdrant_api_key).await?);

        // Initialize graph store
        let graph_path = config.storage.data_dir.join("graph.redb");
        let graph_store = Arc::new(GraphStore::new(&graph_path)?);

        // Initialize text store
        let text_path = config.storage.data_dir.join("tantivy");
        let text_store = Arc::new(TextStore::new(&text_path)?);

        Ok(Self {
            config,
            vector_store,
            graph_store,
            text_store,
            embedder,
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

        // Generate embedding
        let embedding = self.embedder.embed(&[&req.content]).await?
            .into_iter()
            .next()
            .ok_or_else(|| PerspectiveError::Embedding("No embedding returned".into()))?;

        // Ensure collection exists
        let dims = self.embedder.dimensions() as u64;
        self.vector_store.ensure_collection(&req.tenant_id, dims).await?;

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

        // Store in Qdrant
        let payload = serde_json::json!({
            "tenant_id": req.tenant_id,
            "memory_type": memory.memory_type(),
            "content": req.content,
            "tags": req.tags,
            "created_at": now.to_rfc3339(),
        });
        self.vector_store.upsert(&req.tenant_id, id, embedding, payload).await?;

        // Store in graph
        self.graph_store.save_node(
            &req.tenant_id,
            &GraphNode::MemoryRef { id, memory_type: req.memory_type },
        )?;

        // Store in text index
        self.text_store.add_document(&req.tenant_id, id, &req.content)?;

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

        // 1. Vector search
        let query_embedding = self.embedder.embed(&[query]).await?
            .into_iter()
            .next()
            .ok_or_else(|| PerspectiveError::Embedding("No embedding returned".into()))?;

        let vector_results = self.vector_store.search(tenant_id, query_embedding, overfetch as u64).await?;

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

        // 5. Apply recency × importance scoring
        let mut results = Vec::new();
        let mut result_scores = Vec::new();

        for (id, _base_score) in sorted {
            // For now, return the base RRF score
            // Full scoring will be added with decay system
            results.push(id);
            result_scores.push(_base_score);
        }

        // 6. Load full memories from Qdrant payloads
        let mut memories = Vec::new();
        for (id, _score) in results.iter().zip(result_scores.iter()) {
            // Search for this specific ID
            if let Ok(search_results) = self.vector_store.search(
                tenant_id,
                vec![0.0; self.embedder.dimensions()], // dummy vector
                1,
            ).await {
                if let Some(sr) = search_results.iter().find(|r| r.id == *id) {
                    if let Some(payload) = &sr.payload {
                        let _memory_type = payload.get("memory_type")
                            .and_then(|v| v.as_str())
                            .and_then(|s| match s {
                                "episodic" => Some(MemoryType::Episodic),
                                "semantic" => Some(MemoryType::Semantic),
                                "procedural" => Some(MemoryType::Procedural),
                                _ => None,
                            })
                            .unwrap_or(MemoryType::Episodic);
                        let content = payload.get("content")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let tags: Vec<String> = payload.get("tags")
                            .and_then(|v| serde_json::from_value(v.clone()).ok())
                            .unwrap_or_default();

                        memories.push(Memory::Episodic(EpisodicMemory {
                            base: MemoryBase {
                                id: *id,
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
        }

        Ok(RecallResult {
            memories,
            scores: result_scores,
        })
    }

    /// Get a specific memory by ID.
    pub async fn get_memory(&self, tenant_id: &str, id: Uuid) -> Result<Memory> {
        let results = self.vector_store.search(
            tenant_id,
            vec![0.0; self.embedder.dimensions()],
            100,
        ).await?;

        for sr in results {
            if sr.id == id {
                if let Some(payload) = &sr.payload {
                    let content = payload.get("content")
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
        self.vector_store.delete(tenant_id, id).await?;
        self.text_store.delete_document(tenant_id, id)?;
        // Graph nodes are not deleted (keep for referential integrity)
        Ok(())
    }

    /// List tenants.
    pub async fn list_tenants(&self) -> Result<Vec<String>> {
        // Qdrant doesn't have a list collections API in the client
        // For now return empty - will be implemented with proper tenant tracking
        Ok(vec![])
    }
}
