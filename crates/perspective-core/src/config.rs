use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub embedding: EmbeddingConfig,
    pub extraction: ExtractionConfig,
    pub decay: DecayConfig,
    pub consolidation: ConsolidationConfig,
    pub storage: StorageConfig,
    pub retrieval: RetrievalConfig,
    #[serde(default)]
    pub dashboard_port: Option<u16>,
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
    /// Path to the bundled GGUF model file. Used when endpoint is empty.
    pub model_path: String,
    /// Max tokens to generate per extraction call.
    pub max_tokens: u32,
    /// Context window size for the local model.
    pub n_ctx: u32,
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
                // Empty endpoint = use bundled model (Ternary-Bonsai-1.7B).
                // Set to a URL to use an external OpenAI-compatible server instead.
                endpoint: String::new(),
                model: "Ternary-Bonsai-1.7B-Q2_0".into(),
                api_key: None,
                batch_size: 10,
                batch_interval_secs: 30,
                importance_gate: true,
                model_path: "models/Ternary-Bonsai-1.7B-Q2_0.gguf".into(),
                max_tokens: 256,
                n_ctx: 2048,
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
            dashboard_port: Some(2085),
        }
    }
}
