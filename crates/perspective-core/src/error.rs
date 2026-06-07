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
