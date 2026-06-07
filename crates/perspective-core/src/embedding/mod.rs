use crate::error::{PerspectiveError, Result};
use async_trait::async_trait;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

#[async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn dimensions(&self) -> usize;
    fn model_name(&self) -> &str;
}

pub struct LocalEmbedder {
    model: TextEmbedding,
    dimensions: usize,
    model_name: String,
}

impl LocalEmbedder {
    pub fn new(model_name: &str) -> Result<Self> {
        let model = match model_name {
            "all-MiniLM-L6-v2" => {
                TextEmbedding::try_new(InitOptions::new(EmbeddingModel::AllMiniLML6V2))
            }
            _ => {
                return Err(PerspectiveError::Embedding(format!(
                    "Unknown model: {}. Supported: all-MiniLM-L6-v2",
                    model_name
                )))
            }
        }
        .map_err(|e| PerspectiveError::Embedding(e.to_string()))?;

        let model_info = TextEmbedding::get_model_info(&EmbeddingModel::AllMiniLML6V2)
            .map_err(|e| PerspectiveError::Embedding(e.to_string()))?;

        let dimensions = model_info.dim;
        Ok(Self {
            model,
            dimensions,
            model_name: model_name.into(),
        })
    }
}

#[async_trait]
impl Embedder for LocalEmbedder {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let strings: Vec<String> = texts.iter().map(|s| s.to_string()).collect();
        let embeddings = self
            .model
            .embed(strings, None)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_local_embedder() {
        let embedder = LocalEmbedder::new("all-MiniLM-L6-v2").unwrap();
        assert!(embedder.dimensions() > 0);

        let texts = vec!["Hello world", "Test embedding"];
        let embeddings = embedder.embed(&texts).await.unwrap();
        assert_eq!(embeddings.len(), 2);
        assert_eq!(embeddings[0].len(), embedder.dimensions());
    }
}
