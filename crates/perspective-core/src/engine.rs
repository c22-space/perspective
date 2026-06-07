use std::sync::Arc;
use crate::config::Config;
use crate::error::Result;
use crate::embedding::local::LocalEmbedder;
use crate::embedding::Embedder;

pub struct PerspectiveEngine {
    config: Config,
    // vector_store: Arc<QdrantVectorStore>,
    // graph_store: Arc<GraphStore>,
    // text_store: Arc<TextStore>,
    embedder: Arc<dyn Embedder>,
}

impl PerspectiveEngine {
    pub async fn new(config: Config) -> Result<Self> {
        // Initialize embedder
        let embedder: Arc<dyn Embedder> = match &config.embedding {
            crate::config::EmbeddingConfig::Local { model } => {
                Arc::new(LocalEmbedder::new(model)?)
            }
            _ => return Err(crate::error::PerspectiveError::Config(
                "API embeddings not yet implemented".into(),
            )),
        };

        // TODO: Initialize Qdrant vector store
        // TODO: Initialize redb graph store
        // TODO: Initialize Tantivy text store

        Ok(Self {
            config,
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
