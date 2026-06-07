use std::collections::HashMap;
use std::sync::Arc;

use ::perspective_core::config::Config;
use ::perspective_core::engine::{PerspectiveEngine as CoreEngine, StoreRequest};
use ::perspective_core::types::MemoryType;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use tokio::runtime::Runtime;

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
///
/// Usage:
///     engine = PerspectiveEngine(data_dir="/path/to/data")
///     id = engine.store(tenant_id="my_agent", content="Hello world", memory_type="episodic")
///     results = engine.recall(tenant_id="my_agent", query="greeting", budget=10)
///     for r in results:
///         print(f"{r.content} (score={r.score})")
#[pyclass]
struct PerspectiveEngine {
    inner: CoreEngine,
    runtime: Arc<Runtime>,
}

#[pymethods]
impl PerspectiveEngine {
    /// Create a new engine with default config.
    #[new]
    #[pyo3(signature = (data_dir,))]
    fn py_new(data_dir: &str) -> PyResult<Self> {
        let mut config = Config::default();
        config.storage.data_dir = std::path::PathBuf::from(data_dir);

        let engine = CoreEngine::new(config).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "Failed to create engine: {e}"
            ))
        })?;

        let runtime = Runtime::new().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "Failed to create tokio runtime: {e}"
            ))
        })?;

        Ok(Self {
            inner: engine,
            runtime: Arc::new(runtime),
        })
    }

    /// Store a memory.
    ///
    /// Args:
    ///     tenant_id: Tenant identifier.
    ///     content: Memory content text.
    ///     memory_type: "episodic", "semantic", or "procedural".
    ///     tags: Optional list of tags.
    ///     context: Optional context string.
    ///     session_id: Optional session identifier.
    ///
    /// Returns:
    ///     Memory ID as a string.
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
        };

        let engine = &self.inner;
        let id = self
            .runtime
            .block_on(async { engine.store(req).await })
            .map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Store failed: {e}"))
            })?;

        Ok(id.to_string())
    }

    /// Recall relevant memories for a query.
    ///
    /// Args:
    ///     tenant_id: Tenant identifier.
    ///     query: Search query.
    ///     budget: Maximum number of results (default: 10).
    ///
    /// Returns:
    ///     List of MemoryResult objects.
    #[pyo3(signature = (tenant_id, query, budget=10))]
    fn recall(
        &self,
        tenant_id: &str,
        query: &str,
        budget: usize,
    ) -> PyResult<Vec<MemoryResult>> {
        let result = self
            .runtime
            .block_on(async { self.inner.recall(tenant_id, query, budget).await })
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

    /// Reflect on a query: recall relevant memories and return them grouped.
    ///
    /// Args:
    ///     tenant_id: Tenant identifier.
    ///     query: Reflection query.
    ///
    /// Returns:
    ///     Dict with keys "episodic", "semantic", "procedural" containing lists of dicts.
    fn reflect(
        &self,
        py: Python<'_>,
        tenant_id: &str,
        query: &str,
    ) -> PyResult<PyObject> {
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

    /// Delete a memory by ID.
    #[pyo3(signature = (tenant_id, memory_id))]
    fn delete(&self, tenant_id: &str, memory_id: &str) -> PyResult<()> {
        let id: uuid::Uuid = memory_id.parse().map_err(|_| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>(format!("Invalid UUID: {memory_id}"))
        })?;

        self.runtime
            .block_on(async { self.inner.delete_memory(tenant_id, id).await })
            .map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Delete failed: {e}"))
            })?;

        Ok(())
    }

    /// Health check.
    fn health(&self) -> PyResult<bool> {
        let result = self
            .runtime
            .block_on(async { self.inner.recall("default", "health_check", 1).await });
        Ok(result.is_ok())
    }

    /// List tenants.
    fn list_tenants(&self) -> PyResult<Vec<String>> {
        self.runtime
            .block_on(async { self.inner.list_tenants().await })
            .map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                    "Failed to list tenants: {e}"
                ))
            })
    }
}

/// Perspective memory engine Python module.
#[pymodule]
fn perspective_python(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PerspectiveEngine>()?;
    m.add_class::<MemoryResult>()?;
    Ok(())
}
