use std::collections::HashMap;
use std::sync::{Arc, Once};

use ::perspective_core::config::Config;
use ::perspective_core::engine::{PerspectiveEngine as CoreEngine, StoreRequest};
use ::perspective_core::types::MemoryType;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use tokio::runtime::Runtime;

/// Ensure the tracing subscriber is only initialized once per process.
static LOGGING_INIT: Once = Once::new();

/// Recursively copy a directory tree from src to dst.
fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

/// Initialize tracing to log to both stdout and a file in data_dir.
fn init_logging(data_dir: &str) {
    LOGGING_INIT.call_once(|| {
        let log_dir = std::path::PathBuf::from(data_dir);
        let _ = std::fs::create_dir_all(&log_dir);
        let log_path = log_dir.join("perspective.log");
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .ok();

        use tracing_subscriber::fmt;
        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::util::SubscriberInitExt;

        let stdout_layer = fmt::layer()
            .with_target(false)
            .with_ansi(true);

        if let Some(file) = log_file {
            let file_layer = fmt::layer()
                .with_target(false)
                .with_ansi(false)
                .with_writer(std::sync::Mutex::new(file));

            let _ = tracing_subscriber::registry()
                .with(stdout_layer)
                .with(file_layer)
                .try_init();
            tracing::info!("Logging to {}", log_path.display());
        } else {
            let _ = tracing_subscriber::registry()
                .with(stdout_layer)
                .try_init();
        }
    });
}

/// Python-callable memory result.
#[pyclass]
#[derive(Clone)]
struct MemoryResult {
    #[pyo3(get)]
    id: String,
    #[pyo3(get)]
    content: String,
    #[pyo3(get)]
    memory_type: String,
    #[pyo3(get)]
    score: f32,
    #[pyo3(get)]
    tags: Vec<String>,
}

/// Perspective memory engine.
#[pyclass]
struct PerspectiveEngine {
    inner: Arc<CoreEngine>,
    runtime: Arc<Runtime>,
    _data_dir: String,
}

#[pymethods]
impl PerspectiveEngine {
    #[new]
    #[pyo3(signature = (data_dir, extraction_endpoint=None, extraction_model=None, extraction_api_key=None, extraction_enabled=None, graph_hop_limit=None, retrieval_budget=None))]
    fn py_new(
        data_dir: &str,
        extraction_endpoint: Option<String>,
        extraction_model: Option<String>,
        extraction_api_key: Option<String>,
        extraction_enabled: Option<bool>,
        graph_hop_limit: Option<usize>,
        retrieval_budget: Option<usize>,
    ) -> PyResult<Self> {
        // Initialize logging (once per process)
        init_logging(data_dir);

        let mut config = Config::default();
        config.storage.data_dir = std::path::PathBuf::from(data_dir);

        // Apply extraction overrides
        if let Some(ep) = extraction_endpoint {
            config.extraction.endpoint = ep;
        }
        if let Some(m) = extraction_model {
            config.extraction.model = m;
        }
        if let Some(k) = extraction_api_key {
            config.extraction.api_key = Some(k);
        }
        if let Some(e) = extraction_enabled {
            config.extraction.enabled = e;
        }

        // Apply retrieval overrides
        if let Some(h) = graph_hop_limit {
            config.retrieval.graph_hop_limit = h;
        }
        if let Some(b) = retrieval_budget {
            config.retrieval.default_budget = b;
        }

        let engine = CoreEngine::new(config.clone()).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "Failed to create engine: {e}"
            ))
        })?;

        let runtime = Runtime::new().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "Failed to create tokio runtime: {e}"
            ))
        })?;

        let engine_arc = Arc::new(engine);

        // ── Copy dashboard dist to data_dir/dashboard/ ───────────────────
        let dashboard_src =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../dashboard/dist");
        let dashboard_dst = std::path::PathBuf::from(data_dir).join("dashboard");
        if dashboard_src.exists() && !dashboard_dst.exists() {
            if let Err(e) = copy_dir_all(&dashboard_src, &dashboard_dst) {
                tracing::warn!("Failed to copy dashboard files: {e}");
            } else {
                tracing::info!(
                    "Dashboard copied to {}",
                    dashboard_dst.display()
                );
            }
        }

        // ── Start background HTTP server (must be within tokio runtime) ──
        {
            let engine_for_server = engine_arc.clone();
            let config_for_server = config.clone();
            let dashboard_for_server = dashboard_dst;
            runtime.spawn(async move {
                let server_config = ::perspective_core::server::ServerConfig {
                    host: "127.0.0.1".to_string(),
                    port: 2085,
                    dashboard_dir: Some(dashboard_for_server),
                };
                let server_handle = ::perspective_core::server::start_background_with_config(
                    engine_for_server,
                    config_for_server,
                    server_config,
                );
                let _ = server_handle.await;
            });
        }

        tracing::info!("HTTP server running on http://127.0.0.1:2085");

        Ok(Self {
            inner: engine_arc,
            runtime: Arc::new(runtime),
            _data_dir: data_dir.to_string(),
        })
    }

    #[pyo3(signature = (tenant_id, content, memory_type="episodic", tags=None, context=None, session_id=None))]
    fn store(
        &self,
        tenant_id: &str,
        content: &str,
        memory_type: &str,
        tags: Option<Vec<String>>,
        context: Option<String>,
        session_id: Option<String>,
    ) -> PyResult<String> {
        let mt = match memory_type {
            "episodic" => MemoryType::Episodic,
            "semantic" => MemoryType::Semantic,
            "procedural" => MemoryType::Procedural,
            _ => {
                return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Invalid memory_type: {memory_type}. Use 'episodic', 'semantic', or 'procedural'"
            )))
            }
        };

        let req = StoreRequest {
            tenant_id: tenant_id.to_string(),
            content: content.to_string(),
            memory_type: mt,
            tags: tags.unwrap_or_default(),
            metadata: HashMap::new(),
            context,
            source_session: session_id,
            skip_extraction: false,
        };

        let e = &*self.inner;
        let id = self
            .runtime
            .block_on(async { e.store(req).await })
            .map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Store failed: {e}"))
            })?;

        Ok(id.to_string())
    }

    #[pyo3(signature = (tenant_id, query, budget=10))]
    fn recall(&self, tenant_id: &str, query: &str, budget: usize) -> PyResult<Vec<MemoryResult>> {
        let e = &*self.inner;
        let result = self
            .runtime
            .block_on(async { e.recall(tenant_id, query, budget).await })
            .map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Recall failed: {e}"))
            })?;

        let mut results = Vec::new();
        for (i, memory) in result.memories.iter().enumerate() {
            let score = result.scores.get(i).copied().unwrap_or(0.0);
            let (id, content, mt, tags) = match memory {
                ::perspective_core::types::Memory::Episodic(e) => (
                    e.base.id.to_string(),
                    e.base.content.clone(),
                    "episodic",
                    e.base.tags.clone(),
                ),
                ::perspective_core::types::Memory::Semantic(e) => (
                    e.base.id.to_string(),
                    e.base.content.clone(),
                    "semantic",
                    e.base.tags.clone(),
                ),
                ::perspective_core::types::Memory::Procedural(e) => (
                    e.base.id.to_string(),
                    e.base.content.clone(),
                    "procedural",
                    e.base.tags.clone(),
                ),
            };
            results.push(MemoryResult {
                id,
                content,
                memory_type: mt.to_string(),
                score,
                tags,
            });
        }
        Ok(results)
    }

    fn reflect(&self, py: Python<'_>, tenant_id: &str, query: &str) -> PyResult<PyObject> {
        let results = self.recall(tenant_id, query, 20)?;
        let dict = PyDict::new(py);
        let episodic = PyList::empty(py);
        let semantic = PyList::empty(py);
        let procedural = PyList::empty(py);

        for r in &results {
            let item = PyDict::new(py);
            item.set_item("id", &r.id)?;
            item.set_item("content", &r.content)?;
            item.set_item("score", r.score)?;
            match r.memory_type.as_str() {
                "episodic" => episodic.append(item)?,
                "semantic" => semantic.append(item)?,
                "procedural" => procedural.append(item)?,
                _ => {}
            }
        }
        dict.set_item("episodic", &episodic)?;
        dict.set_item("semantic", &semantic)?;
        dict.set_item("procedural", &procedural)?;
        Ok(dict.into())
    }

    #[pyo3(signature = (tenant_id, memory_id))]
    fn delete(&self, tenant_id: &str, memory_id: &str) -> PyResult<()> {
        let id: uuid::Uuid = memory_id.parse().map_err(|_| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Invalid UUID: {memory_id}"))
        })?;
        let e = &*self.inner;
        self.runtime
            .block_on(async { e.delete_memory(tenant_id, id).await })
            .map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Delete failed: {e}"))
            })?;
        Ok(())
    }

    fn health(&self) -> PyResult<bool> {
        let e = &*self.inner;
        let result = self
            .runtime
            .block_on(async { e.recall("default", "health_check", 1).await });
        Ok(result.is_ok())
    }

    fn list_tenants(&self) -> PyResult<Vec<String>> {
        let e = &*self.inner;
        self.runtime
            .block_on(async { e.list_tenants().await })
            .map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                    "Failed to list tenants: {e}"
                ))
            })
    }

    /// Process a batch of buffered texts through the LLM extraction pipeline.
    /// Returns the number of facts extracted.
    fn process_extraction_batch(&self) -> PyResult<usize> {
        let e = &*self.inner;
        self.runtime
            .block_on(async { e.process_extraction_batch().await })
            .map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Extraction failed: {e}"))
            })
    }

    /// Get the number of memories queued for LLM extraction.
    fn extraction_queue_len(&self) -> usize {
        self.inner.extraction_queue_len()
    }

    /// Run a single consolidation pass (dedup, promotion, community detection).
    #[pyo3(signature = (tenant_id))]
    fn run_consolidation(&self, tenant_id: &str) -> PyResult<String> {
        let e = &*self.inner;
        let report = self
            .runtime
            .block_on(async { e.run_consolidation(tenant_id).await })
            .map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                    "Consolidation failed: {e}"
                ))
            })?;
        serde_json::to_string(&report)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{e}")))
    }

    // --- Dashboard query methods (sync, for querying from Python) ---

    fn status_json(&self) -> String {
        serde_json::to_string(&self.inner.status_response()).unwrap_or_default()
    }

    fn activity_json(&self, limit: usize) -> String {
        serde_json::to_string(&self.inner.get_activity(limit)).unwrap_or_default()
    }

    fn processes_json(&self) -> String {
        serde_json::to_string(&self.inner.get_processes()).unwrap_or_default()
    }

    fn graph_json(&self) -> String {
        serde_json::to_string(&self.inner.get_graph_stats()).unwrap_or_default()
    }

    fn config_json(&self) -> String {
        serde_json::to_string(&self.inner.get_config_response()).unwrap_or_default()
    }

    fn memories_json(&self, tenant_id: &str, query: &str, limit: usize) -> String {
        serde_json::to_string(&self.inner.list_memories(tenant_id, query, limit))
            .unwrap_or_default()
    }
}

/// Perspective memory engine Python module.
#[pymodule]
fn perspective_python(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PerspectiveEngine>()?;
    m.add_class::<MemoryResult>()?;
    Ok(())
}
