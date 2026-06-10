use crate::config::Config;
use crate::engine::{PerspectiveEngine, StoreRequest};
use crate::types::MemoryType;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

// ── API Request / Response types ──────────────────────────────────────────────

#[derive(Deserialize)]
struct StoreApiRequest {
    tenant_id: String,
    content: String,
    memory_type: MemoryType,
    tags: Option<Vec<String>>,
    context: Option<String>,
    session_id: Option<String>,
}

#[derive(Serialize)]
struct StoreApiResponse {
    id: String,
}

#[derive(Deserialize)]
struct RecallApiRequest {
    tenant_id: String,
    query: String,
    budget: Option<usize>,
}

#[derive(Serialize)]
struct RecallMemoryItem {
    id: String,
    content: String,
    #[serde(rename = "type")]
    memory_type: String,
    score: f32,
}

#[derive(Serialize)]
struct RecallApiResponse {
    memories: Vec<RecallMemoryItem>,
    count: usize,
}

#[derive(Deserialize)]
struct ReflectApiRequest {
    tenant_id: String,
    query: String,
}

#[derive(Serialize)]
struct ReflectApiResponse {
    synthesis: String,
}

#[derive(Serialize)]
struct HealthApiResponse {
    healthy: bool,
    version: String,
}

#[derive(Serialize)]
struct TenantsApiResponse {
    tenants: Vec<String>,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Serialize)]
pub struct LogsApiResponse {
    pub lines: Vec<String>,
    pub total: usize,
    pub log_path: String,
}

/// Machine-readable status payload.
#[derive(Serialize)]
pub struct StatusPayload {
    pub health: String,
    pub uptime_secs: u64,
    pub total_memories: u64,
    pub tenant_count: u64,
    pub memory_types: StatusMemoryTypeCounts,
    pub gc_candidates: u64,
    pub decay_config: DecayConfigView,
    pub recent_activity: Vec<ActivityEntry>,
}

#[derive(Serialize)]
pub struct StatusMemoryTypeCounts {
    pub episodic: u64,
    pub semantic: u64,
    pub procedural: u64,
}

#[derive(Serialize)]
pub struct DecayConfigView {
    pub episodic_lambda: f32,
    pub semantic_lambda: f32,
    pub procedural_lambda: f32,
    pub learning_rate: f32,
    pub retrieval_threshold: f32,
    pub gc_threshold: f32,
}

#[derive(Serialize)]
pub struct ActivityEntry {
    pub timestamp: String,
    pub tenant_id: String,
    pub memory_type: String,
    pub content: String,
}

// ── Helper: extract body from raw HTTP request ────────────────────────────────

fn extract_body(request: &str) -> &str {
    // Find the blank line separating headers from body
    if let Some(pos) = request.find("\r\n\r\n") {
        &request[pos + 4..]
    } else if let Some(pos) = request.find("\n\n") {
        &request[pos + 2..]
    } else {
        ""
    }
}

/// Memory type as a string for API responses.
fn memory_type_str(mt: &MemoryType) -> &'static str {
    match mt {
        MemoryType::Episodic => "episodic",
        MemoryType::Semantic => "semantic",
        MemoryType::Procedural => "procedural",
    }
}

/// Build status from live engine data.
pub async fn build_status(engine: &PerspectiveEngine, config: &Config) -> StatusPayload {
    let status = engine.status_response();
    let activity = engine.get_activity(20);
    StatusPayload {
        health: status.health,
        uptime_secs: status.uptime_secs,
        total_memories: status.total_memories,
        tenant_count: 0,
        memory_types: StatusMemoryTypeCounts {
            episodic: status.memory_types.episodic,
            semantic: status.memory_types.semantic,
            procedural: status.memory_types.procedural,
        },
        gc_candidates: status.gc_candidates,
        decay_config: DecayConfigView {
            episodic_lambda: config.decay.episodic_lambda,
            semantic_lambda: config.decay.semantic_lambda,
            procedural_lambda: config.decay.procedural_lambda,
            learning_rate: config.decay.learning_rate,
            retrieval_threshold: config.decay.retrieval_threshold,
            gc_threshold: config.decay.gc_threshold,
        },
        recent_activity: activity
            .events
            .iter()
            .map(|e| ActivityEntry {
                timestamp: e.timestamp.to_rfc3339(),
                tenant_id: String::new(),
                memory_type: e.memory_type.clone().unwrap_or_default(),
                content: e.content.clone().unwrap_or_default(),
            })
            .collect(),
    }
}

// ── Handler functions ─────────────────────────────────────────────────────────

/// Handle a POST /api/store request.
async fn handle_store(engine: &PerspectiveEngine, body: &str) -> (String, String) {
    let req: StoreApiRequest = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Store request parse error: {e}");
            let resp = ErrorResponse {
                error: format!("Invalid request body: {e}"),
            };
            return (
                "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                serde_json::to_string(&resp).unwrap_or_default(),
            );
        }
    };

    let tags = req.tags.unwrap_or_default();
    let metadata = std::collections::HashMap::new();

    let store_req = StoreRequest {
        tenant_id: req.tenant_id,
        content: req.content,
        memory_type: req.memory_type,
        tags,
        metadata,
        context: req.context,
        source_session: req.session_id,
        skip_extraction: false,
    };

    let store_tenant = store_req.tenant_id.clone();

    match engine.store(store_req).await {
        Ok(id) => {
            tracing::info!("Stored memory {id} (tenant={store_tenant})");
            let resp = StoreApiResponse { id: id.to_string() };
            (
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                serde_json::to_string(&resp).unwrap_or_default(),
            )
        }
        Err(e) => {
            tracing::warn!("store: failed: {e}");
            let resp = ErrorResponse {
                error: format!("Store failed: {e}"),
            };
            (
                "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                serde_json::to_string(&resp).unwrap_or_default(),
            )
        }
    }
}

/// Handle a POST /api/recall request.
async fn handle_recall(engine: &PerspectiveEngine, body: &str) -> (String, String) {
    let req: RecallApiRequest = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("recall: parse error: {e}");
            let resp = ErrorResponse {
                error: format!("Invalid request body: {e}"),
            };
            return (
                "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                serde_json::to_string(&resp).unwrap_or_default(),
            );
        }
    };

    let budget = req.budget.unwrap_or(10);
    tracing::info!(
        "recall: tenant={} query=\"{}\" budget={budget}",
        req.tenant_id,
        &req.query.chars().take(80).collect::<String>(),
    );

    match engine.recall(&req.tenant_id, &req.query, budget).await {
        Ok(result) => {
            tracing::info!("recall: returned {} results", result.memories.len());
            let memories: Vec<RecallMemoryItem> = result
                .memories
                .iter()
                .zip(result.scores.iter())
                .map(|(m, score)| {
                    let (id, content, mt) = match m {
                        crate::types::Memory::Episodic(e) => (
                            e.base.id,
                            e.base.content.clone(),
                            memory_type_str(&MemoryType::Episodic),
                        ),
                        crate::types::Memory::Semantic(e) => (
                            e.base.id,
                            e.base.content.clone(),
                            memory_type_str(&MemoryType::Semantic),
                        ),
                        crate::types::Memory::Procedural(e) => (
                            e.base.id,
                            e.base.content.clone(),
                            memory_type_str(&MemoryType::Procedural),
                        ),
                    };
                    RecallMemoryItem {
                        id: id.to_string(),
                        content,
                        memory_type: mt.to_string(),
                        score: *score,
                    }
                })
                .collect();

            let count = memories.len();
            let resp = RecallApiResponse { memories, count };
            (
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                serde_json::to_string(&resp).unwrap_or_default(),
            )
        }
        Err(e) => {
            tracing::warn!("recall: failed: {e}");
            let resp = ErrorResponse {
                error: format!("Recall failed: {e}"),
            };
            (
                "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                serde_json::to_string(&resp).unwrap_or_default(),
            )
        }
    }
}

/// Handle a POST /api/reflect request.
async fn handle_reflect(engine: &PerspectiveEngine, body: &str) -> (String, String) {
    let req: ReflectApiRequest = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("reflect: parse error: {e}");
            let resp = ErrorResponse {
                error: format!("Invalid request body: {e}"),
            };
            return (
                "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                serde_json::to_string(&resp).unwrap_or_default(),
            );
        }
    };

    tracing::info!(
        "reflect: tenant={} query=\"{}\"",
        req.tenant_id,
        &req.query.chars().take(80).collect::<String>(),
    );
    // Recall relevant memories to build a synthesis
    let synthesis = match engine.recall(&req.tenant_id, &req.query, 5).await {
        Ok(result) => {
            if result.memories.is_empty() {
                format!(
                    "No memories found for query '{}' in tenant '{}'. \
                     The engine has no stored context to reflect upon.",
                    req.query, req.tenant_id
                )
            } else {
                let summaries: Vec<String> = result
                    .memories
                    .iter()
                    .map(|m| {
                        let content = match m {
                            crate::types::Memory::Episodic(e) => &e.base.content,
                            crate::types::Memory::Semantic(e) => &e.base.content,
                            crate::types::Memory::Procedural(e) => &e.base.content,
                        };
                        content.chars().take(200).collect::<String>()
                    })
                    .collect();

                format!(
                    "Reflection on '{}' (tenant '{}'): found {} relevant memories. \
                     Key themes: {}. \
                     Summary of retrieved context: [{}]",
                    req.query,
                    req.tenant_id,
                    result.memories.len(),
                    summaries.len(), // count of themes
                    summaries.join(" | ")
                )
            }
        }
        Err(e) => {
            format!("Reflection failed: {e}")
        }
    };

    let resp = ReflectApiResponse { synthesis };
    (
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n"
            .into(),
        serde_json::to_string(&resp).unwrap_or_default(),
    )
}

/// Handle a GET /api/logs request.
/// Reads the last N lines from the perspective.log file.
/// Supports ?limit=N (default 100) and ?filter=keyword query params.
fn handle_logs(log_dir: &std::path::Path, query: &str) -> (String, String) {
    let log_path = log_dir.join("perspective.log");

    let (limit, filter) = if let Some(qs) = query.split_once('?') {
        let params: std::collections::HashMap<&str, &str> =
            qs.1.split('&').filter_map(|p| p.split_once('=')).collect();
        (
            params
                .get("limit")
                .and_then(|l| l.parse::<usize>().ok())
                .unwrap_or(100),
            params.get("filter").unwrap_or(&"").to_string(),
        )
    } else {
        (100, String::new())
    };

    match std::fs::read_to_string(&log_path) {
        Ok(content) => {
            let mut lines: Vec<String> = content.lines().map(String::from).collect();
            // Apply filter
            if !filter.is_empty() {
                lines = lines
                    .into_iter()
                    .filter(|l| l.to_lowercase().contains(&filter.to_lowercase()))
                    .collect();
            }
            // Take last N lines
            let total = lines.len();
            if lines.len() > limit {
                lines = lines.split_off(lines.len() - limit);
            }
            let resp = LogsApiResponse {
                lines,
                total,
                log_path: log_path.display().to_string(),
            };
            (
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                serde_json::to_string(&resp).unwrap_or_default(),
            )
        }
        Err(e) => {
            let resp = LogsApiResponse {
                lines: vec![format!("Log file not available: {e}")],
                total: 0,
                log_path: log_path.display().to_string(),
            };
            (
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                serde_json::to_string(&resp).unwrap_or_default(),
            )
        }
    }
}

/// Handle a GET /api/health request.
fn handle_health() -> (String, String) {
    let resp = HealthApiResponse {
        healthy: true,
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    (
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n"
            .into(),
        serde_json::to_string(&resp).unwrap_or_default(),
    )
}

/// Handle a GET /api/tenants request.
async fn handle_tenants(engine: &PerspectiveEngine) -> (String, String) {
    match engine.list_tenants().await {
        Ok(tenants) => {
            tracing::debug!("tenants: {} tenants", tenants.len());
            let resp = TenantsApiResponse { tenants };
            (
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                serde_json::to_string(&resp).unwrap_or_default(),
            )
        }
        Err(e) => {
            tracing::warn!("tenants: failed: {e}");
            let resp = ErrorResponse {
                error: format!("Failed to list tenants: {e}"),
            };
            (
                "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                serde_json::to_string(&resp).unwrap_or_default(),
            )
        }
    }
}

// ── HTTP Server ───────────────────────────────────────────────────────────────

/// Configuration for the background HTTP server.
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub dashboard_dir: Option<std::path::PathBuf>,
}

/// Start the HTTP server in a background tokio task on port 2085.
///
/// This is the simple entry point: uses defaults from the engine Config.
/// Returns a `JoinHandle` for the spawned task.
pub fn start_background(
    engine: Arc<PerspectiveEngine>,
    config: Config,
) -> JoinHandle<()> {
    let server_config = ServerConfig {
        host: "0.0.0.0".to_string(),
        port: 2085,
        dashboard_dir: None,
    };
    start_background_with_config(engine, config, server_config)
}

/// Start the HTTP server in a background tokio task with custom server config.
///
/// Returns a `JoinHandle` for the spawned task.
pub fn start_background_with_config(
    engine: Arc<PerspectiveEngine>,
    config: Config,
    server_config: ServerConfig,
) -> JoinHandle<()> {
    let dashboard_dir = server_config.dashboard_dir.unwrap_or_else(|| {
        // CI copies dashboard/dist into this crate before build
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("dashboard_dist")
    });
    let static_files = crate::static_files::StaticFiles::new(dashboard_dir);
    let log_dir = config.storage.data_dir.clone();

    let host = server_config.host;
    let port = server_config.port;

    tokio::spawn(async move {
        let addr = format!("{host}:{port}");
        let listener = match TcpListener::bind(addr.as_str()).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Failed to bind HTTP server on {addr}: {e}");
                return;
            }
        };
        tracing::info!("HTTP server started on {addr}");

        loop {
            let (mut stream, _remote) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => continue,
            };

            let mut buf = vec![0u8; 65536];
            let mut total = 0;
            loop {
                let n = match stream.read(&mut buf[total..]).await {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(_) => break,
                };
                total += n;
                // Check if we have a complete request (double newline after headers)
                let so_far = String::from_utf8_lossy(&buf[..total]);
                if so_far.contains("\r\n\r\n") || so_far.contains("\n\n") {
                    // For POST requests, also need the body
                    let headers_end = if let Some(pos) = so_far.find("\r\n\r\n") {
                        pos + 4
                    } else if let Some(pos) = so_far.find("\n\n") {
                        pos + 2
                    } else {
                        0
                    };
                    let headers = &so_far[..headers_end];
                    // Parse Content-Length
                    let content_length: usize = headers
                        .lines()
                        .find(|l| l.to_lowercase().starts_with("content-length:"))
                        .and_then(|l| l.split(':').nth(1))
                        .and_then(|v| v.trim().parse().ok())
                        .unwrap_or(0);
                    let body_received = total - headers_end;
                    if content_length == 0 || body_received >= content_length {
                        break;
                    }
                    // Need more data, keep reading
                }
                if total >= buf.len() {
                    break;
                }
            }
            let request = String::from_utf8_lossy(&buf[..total]);

            // ── Determine method and path ──────────────────────
            let first_line = request.lines().next().unwrap_or("");
            let parts: Vec<&str> = first_line.split_whitespace().collect();
            let method = parts.first().copied().unwrap_or("");
            let path = parts.get(1).copied().unwrap_or("/");
            tracing::debug!("{} {}", method, path);

            let (status_line, body) = if method == "OPTIONS" {
                // CORS preflight
                (
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\
                     Access-Control-Allow-Origin: *\r\n\
                     Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
                     Access-Control-Allow-Headers: Content-Type\r\n\
                     Content-Length: 0\r\nConnection: close\r\n\r\n"
                        .into(),
                    String::new(),
                )
            } else if method == "GET" && path == "/api/health" {
                handle_health()
            } else if method == "GET" && path == "/api/tenants" {
                handle_tenants(&engine).await
            } else if method == "GET" && path == "/api/status" {
                let st = build_status(&engine, &config).await;
                let json = serde_json::to_string(&st).unwrap_or_default();
                (
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                    json,
                )
            } else if method == "POST" && path == "/api/store" {
                let body_str = extract_body(&request).to_string();
                handle_store(&engine, &body_str).await
            } else if method == "POST" && path == "/api/recall" {
                let body_str = extract_body(&request).to_string();
                handle_recall(&engine, &body_str).await
            } else if method == "POST" && path == "/api/reflect" {
                let body_str = extract_body(&request).to_string();
                handle_reflect(&engine, &body_str).await
            } else if method == "GET" && path.starts_with("/api/activity") {
                // Check if path has a numeric ID: /api/activity/123
                let after_activity = &path["/api/activity".len()..];
                if after_activity.starts_with('/') && after_activity.len() > 1 {
                    // /api/activity/:id — single event
                    let id_str = &after_activity[1..];
                    match id_str.parse::<i64>() {
                        Ok(id) => match engine.monitor.get_event(id) {
                            Some(event) => {
                                let json = serde_json::to_string(&event).unwrap_or_default();
                                (
                                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                                    json,
                                )
                            }
                            None => {
                                let resp = ErrorResponse {
                                    error: format!("Event {id} not found"),
                                };
                                (
                                    "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                                    serde_json::to_string(&resp).unwrap_or_default(),
                                )
                            }
                        },
                        Err(_) => {
                            let resp = ErrorResponse {
                                error: "Invalid event ID".into(),
                            };
                            (
                                "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                                serde_json::to_string(&resp).unwrap_or_default(),
                            )
                        }
                    }
                } else {
                    // /api/activity — list events
                    let resp = engine.get_activity(50);
                    let json = serde_json::to_string(&resp).unwrap_or_default();
                    (
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                        json,
                    )
                }
            } else if method == "GET" && path == "/api/processes" {
                let resp = engine.get_processes();
                let json = serde_json::to_string(&resp).unwrap_or_default();
                (
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                    json,
                )
            } else if method == "GET" && path.starts_with("/api/logs") {
                handle_logs(&log_dir, path)
            } else if method == "GET" && path == "/api/graph" {
                let resp = engine.get_graph_stats();
                let json = serde_json::to_string(&resp).unwrap_or_default();
                (
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                    json,
                )
            } else if method == "GET" && path == "/api/config" {
                let resp = engine.get_config_response();
                let json = serde_json::to_string(&resp).unwrap_or_default();
                (
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                    json,
                )
            } else if method == "GET" && path.starts_with("/api/memories") {
                // Parse optional ?q= and ?limit= query params
                let (q, limit) = if let Some(qs) = path.split_once('?') {
                    let params: std::collections::HashMap<&str, &str> =
                        qs.1.split('&').filter_map(|p| p.split_once('=')).collect();
                    (
                        params.get("q").unwrap_or(&"").to_string(),
                        params
                            .get("limit")
                            .and_then(|l| l.parse::<usize>().ok())
                            .unwrap_or(50),
                    )
                } else {
                    (String::new(), 50)
                };
                let resp = engine.list_memories("hermes", &q, limit);
                let total = resp.memories.len();
                let wrapped = serde_json::json!({
                    "memories": resp.memories,
                    "total": total,
                });
                let json = serde_json::to_string(&wrapped).unwrap_or_default();
                (
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                    json,
                )
            } else if method == "GET" {
                // Static file serving with SPA fallback
                if let Some(ref sf) = static_files {
                    let (status, ct, body_content) = sf.serve(path);
                    (
                        format!("{status}\r\nContent-Type: {ct}\r\nAccess-Control-Allow-Origin: *\r\n"),
                        body_content,
                    )
                } else {
                    // No dashboard dir configured
                    if path == "/" || path == " " {
                        (
                            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n".into(),
                            "Perspective Memory Engine\nDashboard not configured. Start with --dashboard-dir to serve the React dashboard.".to_string(),
                        )
                    } else {
                        (
                            "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\n"
                                .into(),
                            "Not found".to_string(),
                        )
                    }
                }
            } else {
                (
                    "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\n".into(),
                    "Not found".to_string(),
                )
            };

            let response = format!(
                "{status_line}Content-Length: {len}\r\nConnection: close\r\n\r\n{body}",
                len = body.len(),
            );
            let _ = stream.write_all(response.as_bytes()).await;
        }
    })
}
