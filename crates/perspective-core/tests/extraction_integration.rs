//! Integration tests for fact extraction pipeline using the real bundled model.

use perspective_core::config::{Config, EmbeddingConfig, ExtractionConfig, StorageConfig};
use perspective_core::engine::{PerspectiveEngine, StoreRequest};
use perspective_core::extraction::pipeline::ExtractionPipeline;
use perspective_core::types::MemoryType;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tempfile::TempDir;

/// Serialize tests that load the GGUF model to avoid race conditions.
/// llama.cpp backend init + model load is not safe to run concurrently.
static MODEL_MUTEX: Mutex<()> = Mutex::new(());

/// Resolve the path to the bundled GGUF model relative to the crate directory.
fn bundled_model_path() -> PathBuf {
    // cargo test runs from crates/perspective-core/
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .join("../../models/NuExtract-tiny-v1.5-Q5_K_M.gguf")
        .canonicalize()
        .expect("Bundled model not found at expected path. Run `git lfs pull` first.")
}

fn test_config(temp_dir: &std::path::Path, extraction_enabled: bool) -> Config {
    Config {
        embedding: EmbeddingConfig::Local {
            model: "all-MiniLM-L6-v2".into(),
        },
        extraction: ExtractionConfig {
            enabled: extraction_enabled,
            endpoint: String::new(),
            model: "bundled".into(),
            api_key: None,
            batch_size: 5,
            batch_interval_secs: 0,
            importance_gate: false,
            model_path: bundled_model_path().to_string_lossy().into_owned(),
            max_tokens: 128,
            n_ctx: 512,
        },
        storage: StorageConfig {
            qdrant_url: None,
            qdrant_api_key: None,
            embedded_qdrant: true,
            data_dir: temp_dir.to_path_buf(),
        },
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Real model tests — these load the 442MB GGUF and run inference
// ---------------------------------------------------------------------------

/// Verify the bundled model loads and completes a simple prompt.
#[tokio::test]
async fn test_bundled_model_loads_and_completes() {
    let _lock = MODEL_MUTEX.lock().unwrap();
    let pipeline = ExtractionPipeline::new(ExtractionConfig {
        enabled: true,
        endpoint: String::new(),
        model: "bundled".into(),
        api_key: None,
        batch_size: 5,
        batch_interval_secs: 30,
        importance_gate: false,
        model_path: bundled_model_path().to_string_lossy().into_owned(),
        max_tokens: 128,
        n_ctx: 512,
    });

    assert!(
        pipeline.has_bundled_model(),
        "Bundled model should be available"
    );

    let facts = pipeline
        .extract_batch(&["Alice said the meeting is at 3pm tomorrow"])
        .await
        .unwrap();

    assert_eq!(facts.len(), 1, "Should extract one fact");
    assert!(
        !facts[0].fact.is_empty(),
        "Extracted fact text should not be empty"
    );
    assert!(
        !facts[0].entities.is_empty(),
        "Should extract at least one entity"
    );
}

/// Test extraction of multiple facts in a single batch.
#[tokio::test]
async fn test_batch_extraction_real_model() {
    let _lock = MODEL_MUTEX.lock().unwrap();
    let pipeline = ExtractionPipeline::new(ExtractionConfig {
        enabled: true,
        endpoint: String::new(),
        model: "bundled".into(),
        api_key: None,
        batch_size: 5,
        batch_interval_secs: 30,
        importance_gate: false,
        model_path: bundled_model_path().to_string_lossy().into_owned(),
        max_tokens: 128,
        n_ctx: 512,
    });

    let texts = vec![
        "Alice mentioned the project deadline moved to Friday",
        "Bob prefers dark mode for all his tools",
        "The team decided to use Rust for the backend",
    ];
    let facts = pipeline.extract_batch(&texts).await.unwrap();
    assert_eq!(facts.len(), 3, "Should extract one fact per input text");

    for (i, fact) in facts.iter().enumerate() {
        assert!(
            !fact.fact.is_empty(),
            "Fact {} text should not be empty",
            i
        );
    }
}

/// Test full round-trip: store -> extract -> process_extraction_batch.
/// Verifies extracted facts are stored under the correct tenant.
#[tokio::test]
async fn test_store_extract_roundtrip() {
    let _lock = MODEL_MUTEX.lock().unwrap();
    let temp = TempDir::new().unwrap();
    let config = test_config(temp.path(), true);
    let engine = PerspectiveEngine::new(config).unwrap();

    // Store a document
    let req = StoreRequest {
        tenant_id: "integration-test".to_string(),
        content: "Alice mentioned that the project deadline has been moved to next Friday and she prefers dark mode"
            .to_string(),
        memory_type: MemoryType::Episodic,
        tags: vec![],
        metadata: HashMap::new(),
        context: None,
        source_session: None,
        skip_extraction: false,
    };
    engine.store(req).await.unwrap();

    assert_eq!(
        engine.extraction_queue_len(),
        1,
        "Should have 1 item in extraction queue"
    );

    // Process extraction with real model
    let count = engine.process_extraction_batch().await.unwrap();
    assert!(
        count > 0,
        "Should have stored at least 1 extracted fact, got {}",
        count
    );

    assert_eq!(
        engine.extraction_queue_len(),
        0,
        "Queue should be empty after processing"
    );
}

/// Test that extracted facts are stored under the same tenant as the source.
#[tokio::test]
async fn test_extracted_facts_use_source_tenant() {
    let _lock = MODEL_MUTEX.lock().unwrap();
    let temp = TempDir::new().unwrap();
    let config = test_config(temp.path(), true);
    let engine = PerspectiveEngine::new(config).unwrap();

    // Store docs under a specific tenant
    for i in 0..3 {
        engine
            .store(StoreRequest {
                tenant_id: "my-project".to_string(),
                content: format!(
                    "Doc {}: Alice said the deadline is next Friday and she prefers dark mode",
                    i
                ),
                memory_type: MemoryType::Episodic,
                tags: vec![],
                metadata: HashMap::new(),
                context: None,
                source_session: None,
                skip_extraction: false,
            })
            .await
            .unwrap();
    }

    let count = engine.process_extraction_batch().await.unwrap();
    assert!(
        count > 0,
        "Should extract facts from batch, got {}",
        count
    );

    // Recall from the SAME tenant should find extracted facts
    let results = engine.recall("my-project", "deadline Friday", 10).await.unwrap();
    assert!(
        !results.memories.is_empty(),
        "Recall from 'my-project' should find extracted facts"
    );

    // Recall from a DIFFERENT tenant should NOT find them
    let other_results = engine.recall("other-tenant", "deadline Friday", 10).await.unwrap();
    assert!(
        other_results.memories.is_empty(),
        "Recall from 'other-tenant' should not find facts from 'my-project'"
    );
}

// ---------------------------------------------------------------------------
// Unit-level extraction tests
// ---------------------------------------------------------------------------

/// Test that extraction respects the importance gate (short texts skipped).
#[tokio::test]
async fn test_extraction_respects_importance_gate() {
    let temp = TempDir::new().unwrap();
    let mut config = test_config(temp.path(), true);
    config.extraction.importance_gate = true;
    let engine = PerspectiveEngine::new(config).unwrap();

    engine
        .store(StoreRequest {
            tenant_id: "t".to_string(),
            content: "ok".to_string(),
            memory_type: MemoryType::Episodic,
            tags: vec![],
            metadata: HashMap::new(),
            context: None,
            source_session: None,
            skip_extraction: false,
        })
        .await
        .unwrap();

    assert_eq!(
        engine.extraction_queue_len(),
        0,
        "Importance gate should skip short text"
    );
}

/// Test that skip_extraction flag prevents buffering.
#[tokio::test]
async fn test_extraction_skip_flag() {
    let temp = TempDir::new().unwrap();
    let config = test_config(temp.path(), true);
    let engine = PerspectiveEngine::new(config).unwrap();

    engine
        .store(StoreRequest {
            tenant_id: "t".to_string(),
            content: "Alice prefers dark mode and works on the perspective project".to_string(),
            memory_type: MemoryType::Semantic,
            tags: vec!["extracted".to_string()],
            metadata: HashMap::new(),
            context: None,
            source_session: None,
            skip_extraction: true,
        })
        .await
        .unwrap();

    assert_eq!(
        engine.extraction_queue_len(),
        0,
        "skip_extraction should prevent buffering"
    );
}

/// Test that disabled extraction produces no buffered items.
#[tokio::test]
async fn test_extraction_disabled() {
    let temp = TempDir::new().unwrap();
    let config = test_config(temp.path(), false);
    let engine = PerspectiveEngine::new(config).unwrap();

    engine
        .store(StoreRequest {
            tenant_id: "t".to_string(),
            content: "Alice mentioned the project deadline".to_string(),
            memory_type: MemoryType::Episodic,
            tags: vec![],
            metadata: HashMap::new(),
            context: None,
            source_session: None,
            skip_extraction: false,
        })
        .await
        .unwrap();

    assert_eq!(engine.extraction_queue_len(), 0);
    let count = engine.process_extraction_batch().await.unwrap();
    assert_eq!(count, 0);
}

/// Test missing model falls back to local extraction.
#[tokio::test]
async fn test_extraction_pipeline_missing_model() {
    let pipeline = ExtractionPipeline::new(ExtractionConfig {
        enabled: true,
        endpoint: String::new(),
        model: "test".into(),
        api_key: None,
        batch_size: 10,
        batch_interval_secs: 30,
        importance_gate: false,
        model_path: "/nonexistent/path/model.gguf".into(),
        max_tokens: 256,
        n_ctx: 2048,
    });

    assert!(
        !pipeline.has_bundled_model(),
        "Nonexistent model should not be available"
    );

    let facts = pipeline
        .extract_batch(&["Alice mentioned the project deadline"])
        .await
        .unwrap();

    assert_eq!(facts.len(), 1);
    assert!(!facts[0].entities.is_empty(), "Local entity extraction still works");
}
