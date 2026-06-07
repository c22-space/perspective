use std::collections::HashMap;
use std::sync::Arc;
use std::thread;

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
#[pyclass]
struct PerspectiveEngine {
    inner: Arc<CoreEngine>,
    runtime: Arc<Runtime>,
    _data_dir: String,
    _dashboard_port: Option<u16>,
}

#[pymethods]
impl PerspectiveEngine {
    #[new]
    #[pyo3(signature = (data_dir, dashboard_port=None, dashboard_dist_dir=None))]
    fn py_new(data_dir: &str, dashboard_port: Option<u16>, dashboard_dist_dir: Option<String>) -> PyResult<Self> {
        let mut config = Config::default();
        config.storage.data_dir = std::path::PathBuf::from(data_dir);
        config.dashboard_port = dashboard_port;

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

        // Start dashboard HTTP server in background thread
        if let Some(port) = config.dashboard_port {
            let engine_clone = Arc::clone(&engine_arc);
            let dist_path = dashboard_dist_dir.clone().unwrap_or_default();
            thread::spawn(move || {
                if let Err(e) = run_dashboard_server(engine_clone, &dist_path, port) {
                    eprintln!("Dashboard server error: {e}");
                }
            });
            println!("  Dashboard:    http://127.0.0.1:{port}");
        }

        Ok(Self {
            inner: engine_arc,
            runtime: Arc::new(runtime),
            _data_dir: data_dir.to_string(),
            _dashboard_port: config.dashboard_port,
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
            _ => return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Invalid memory_type: {memory_type}. Use 'episodic', 'semantic', or 'procedural'"
            ))),
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

        let e = &*self.inner;
        let id = self.runtime.block_on(async { e.store(req).await })
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Store failed: {e}")))?;

        Ok(id.to_string())
    }

    #[pyo3(signature = (tenant_id, query, budget=10))]
    fn recall(&self, tenant_id: &str, query: &str, budget: usize) -> PyResult<Vec<MemoryResult>> {
        let e = &*self.inner;
        let result = self.runtime.block_on(async { e.recall(tenant_id, query, budget).await })
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Recall failed: {e}")))?;

        let mut results = Vec::new();
        for (i, memory) in result.memories.iter().enumerate() {
            let score = result.scores.get(i).copied().unwrap_or(0.0);
            let (id, content, mt, tags) = match memory {
                ::perspective_core::types::Memory::Episodic(e) => (
                    e.base.id.to_string(), e.base.content.clone(), "episodic", e.base.tags.clone(),
                ),
                ::perspective_core::types::Memory::Semantic(e) => (
                    e.base.id.to_string(), e.base.content.clone(), "semantic", e.base.tags.clone(),
                ),
                ::perspective_core::types::Memory::Procedural(e) => (
                    e.base.id.to_string(), e.base.content.clone(), "procedural", e.base.tags.clone(),
                ),
            };
            results.push(MemoryResult {
                id, content, memory_type: mt.to_string(), score, tags,
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
        self.runtime.block_on(async { e.delete_memory(tenant_id, id).await })
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Delete failed: {e}")))?;
        Ok(())
    }

    fn health(&self) -> PyResult<bool> {
        let e = &*self.inner;
        let result = self.runtime.block_on(async { e.recall("default", "health_check", 1).await });
        Ok(result.is_ok())
    }

    fn list_tenants(&self) -> PyResult<Vec<String>> {
        let e = &*self.inner;
        self.runtime.block_on(async { e.list_tenants().await })
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("Failed to list tenants: {e}")))
    }

    // --- Dashboard query methods (sync, for HTTP handlers) ---

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
        serde_json::to_string(&self.inner.list_memories(tenant_id, query, limit)).unwrap_or_default()
    }
}

// ============================================================================
// Dashboard HTTP server
// ============================================================================

/// Serve a request from the React dist directory (filesystem).
fn serve_static(dist_dir: &std::path::Path, mut url: &str) -> Option<(Vec<u8>, &'static str)> {
    let mime = |p: &str| -> &'static str {
        if p.ends_with(".js") { "application/javascript" }
        else if p.ends_with(".css") { "text/css" }
        else if p.ends_with(".svg") { "image/svg+xml" }
        else if p.ends_with(".html") { "text/html; charset=utf-8" }
        else if p.ends_with(".json") { "application/json" }
        else if p.ends_with(".png") { "image/png" }
        else if p.ends_with(".woff") || p.ends_with(".woff2") { "font/woff2" }
        else { "application/octet-stream" }
    };
    // Try exact file match first
    if url == "/" { url = "/index.html" }
    let rel = url.trim_start_matches('/');
    let file_path = dist_dir.join(rel);
    if let Ok(data) = std::fs::read(&file_path) {
        return Some((data, mime(rel)));
    }
    // SPA fallback: return index.html for non-file routes
    let index_path = dist_dir.join("index.html");
    if let Ok(data) = std::fs::read(&index_path) {
        return Some((data, "text/html; charset=utf-8"));
    }
    None
}

/// Run the dashboard HTTP server using tiny_http.
fn run_dashboard_server(
    engine: Arc<CoreEngine>,
    dist_dir_arg: &str,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let serve_dir = if !dist_dir_arg.is_empty() {
        std::path::PathBuf::from(dist_dir_arg)
    } else {
        eprintln!("  Dashboard dist not provided. Dashboard disabled.");
        return Ok(());
    };

    let addr = format!("127.0.0.1:{port}");
    let server = tiny_http::Server::http(&addr)
        .map_err(|e| format!("Failed to bind dashboard on {addr}: {e}"))?;

    let cors_headers = [
        "Access-Control-Allow-Origin: *",
        "Access-Control-Allow-Methods: GET, POST, OPTIONS",
        "Access-Control-Allow-Headers: Content-Type",
    ];

    loop {
        match server.recv_timeout(std::time::Duration::from_secs(1)) {
            Ok(Some(request)) => {
                let method = request.method().to_string();
                let url = request.url().to_string();

                // Handle CORS preflight
                if method == "OPTIONS" {
                    let mut resp = tiny_http::Response::from_string("").with_status_code(204);
                    for h in &cors_headers {
                        resp = resp.with_header(hdr(h));
                                                }
                                                let _ = request.respond(resp);
                    continue;
                }

                // API routes -> JSON
                let response_body: serde_json::Value = if url.starts_with("/api/") {
                    match url.as_str() {
                        "/api/status" => serde_json::to_value(engine.status_response()).unwrap_or_default(),
                        "/api/health" => serde_json::json!({"status": "healthy"}),
                        "/api/processes" => serde_json::to_value(engine.get_processes()).unwrap_or_default(),
                        "/api/graph" => serde_json::to_value(engine.get_graph_stats()).unwrap_or_default(),
                        "/api/config" => serde_json::to_value(engine.get_config_response()).unwrap_or_default(),
                        u if u.starts_with("/api/activity") => {
                            let limit = parse_qs(&url, "limit", 50usize);
                            serde_json::to_value(engine.get_activity(limit)).unwrap_or_default()
                        }
                        u if u.starts_with("/api/memories") => {
                            let q = parse_qs_str(&url, "q", "");
                            let limit = parse_qs(&url, "limit", 50usize);
                            serde_json::to_value(engine.list_memories("hermes", q, limit)).unwrap_or_default()
                        }
                        u if u.starts_with("/api/tenants") => {
                            serde_json::json!(["hermes"])  // list_tenants is async, use hardcoded for now
                        }
                        _ => serde_json::json!({"error": "Not found"}),
                    }
                } else {
                    // Static files
                    if let Some((data, ct)) = serve_static(&serve_dir, &url) {
                        let mut resp = tiny_http::Response::from_data(data)
                            .with_header(hdr(&format!("Content-Type: {ct}")));
                        for h in &cors_headers {
                            resp = resp.with_header(hdr(h));
                        }
                        let _ = request.respond(resp);
                        continue;
                    } else {
                        let resp = tiny_http::Response::from_string("Not Found").with_status_code(404);
                        let _ = request.respond(resp);
                        continue;
                    }
                };

                // Send JSON response
                let json_str = serde_json::to_string(&response_body).unwrap_or_default();
                let mut resp = tiny_http::Response::from_string(&json_str)
                    .with_header(hdr("Content-Type: application/json"));
                for h in &cors_headers {
                    resp = resp.with_header(hdr(h));
                }
                let _ = request.respond(resp);
            }
            Ok(None) => continue, // timeout
            Err(e) => {
                eprintln!("Dashboard server error: {e}");
                break;
            }
        }
    }
    Ok(())
}

/// Parse a query string parameter as usize.
fn parse_qs(url: &str, key: &str, default: usize) -> usize {
    url.split('?')
        .nth(1)
        .and_then(|qs| {
            qs.split('&')
                .find(|p| p.starts_with(&format!("{key}=")))
                .and_then(|p| p.split('=').nth(1))
                .and_then(|v| v.parse().ok())
        })
        .unwrap_or(default)
}

/// Build a tiny_http Header from a "Name: Value" string.
fn hdr(s: &str) -> tiny_http::Header {
    let parts: Vec<&str> = s.splitn(2, ':').collect();
    let name = parts[0].trim();
    let value = if parts.len() > 1 { parts[1].trim() } else { "" };
    tiny_http::Header::from_bytes(name.as_bytes(), value.as_bytes()).unwrap()
}

/// Parse a query string parameter as &str.
fn parse_qs_str<'a>(url: &'a str, key: &str, default: &'a str) -> &'a str {
    url.split('?')
        .nth(1)
        .and_then(|qs| {
            qs.split('&')
                .find(|p| p.starts_with(&format!("{key}=")))
                .and_then(|p| p.split('=').nth(1))
        })
        .unwrap_or(default)
}

/// Perspective memory engine Python module.
#[pymodule]
fn perspective_python(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PerspectiveEngine>()?;
    m.add_class::<MemoryResult>()?;
    Ok(())
}
