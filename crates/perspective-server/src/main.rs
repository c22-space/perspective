mod dashboard;
mod static_files;

use clap::{Parser, Subcommand};
use perspective_core::config::Config;
use perspective_core::engine::{PerspectiveEngine, StoreRequest};
use perspective_core::types::MemoryType;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(
    name = "perspective",
    version,
    about = "Perspective memory engine — graph+vector memory for AI agents"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Path to config file (TOML)
    #[arg(short, long, global = true)]
    config: Option<String>,

    /// Data directory override
    #[arg(short = 'd', long, global = true)]
    data_dir: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the HTTP server and dashboard
    Serve {
        /// Bind address
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// HTTP dashboard port (0 to disable)
        #[arg(long, default_value = "8080")]
        dashboard_port: u16,

        /// Path to React dashboard dist directory (for serving static files)
        #[arg(long)]
        dashboard_dir: Option<PathBuf>,
    },

    /// Show engine status
    Status {
        /// Show detail for a specific tenant
        #[arg(short, long)]
        tenant: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Initialize data directory and default config
    Init {
        /// Directory to initialize (default: ./perspective-data)
        #[arg(short, long)]
        dir: Option<PathBuf>,

        /// Force overwrite existing config
        #[arg(long)]
        force: bool,
    },

    /// Show current configuration
    Config {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

/// Machine-readable status payload.
#[derive(Serialize)]
struct StatusPayload {
    health: String,
    uptime_secs: u64,
    total_memories: u64,
    tenant_count: u64,
    memory_types: MemoryTypeCounts,
    gc_candidates: u64,
    decay_config: DecayConfigView,
    recent_activity: Vec<ActivityEntry>,
}

#[derive(Serialize)]
struct MemoryTypeCounts {
    episodic: u64,
    semantic: u64,
    procedural: u64,
}

#[derive(Serialize)]
struct DecayConfigView {
    episodic_lambda: f32,
    semantic_lambda: f32,
    procedural_lambda: f32,
    learning_rate: f32,
    retrieval_threshold: f32,
    gc_threshold: f32,
}

#[derive(Serialize)]
struct ActivityEntry {
    timestamp: String,
    tenant_id: String,
    memory_type: String,
    content: String,
}

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
struct ErrorResponse {
    error: String,
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

/// Handle a POST /api/store request.
async fn handle_store(engine: &PerspectiveEngine, body: &str) -> (String, String) {
    let req: StoreApiRequest = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(e) => {
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

    match engine.store(store_req).await {
        Ok(id) => {
            let resp = StoreApiResponse { id: id.to_string() };
            (
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                serde_json::to_string(&resp).unwrap_or_default(),
            )
        }
        Err(e) => {
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

    match engine.recall(&req.tenant_id, &req.query, budget).await {
        Ok(result) => {
            let memories: Vec<RecallMemoryItem> = result
                .memories
                .iter()
                .zip(result.scores.iter())
                .map(|(m, score)| {
                    let (id, content, mt) = match m {
                        perspective_core::types::Memory::Episodic(e) => (
                            e.base.id,
                            e.base.content.clone(),
                            memory_type_str(&MemoryType::Episodic),
                        ),
                        perspective_core::types::Memory::Semantic(e) => (
                            e.base.id,
                            e.base.content.clone(),
                            memory_type_str(&MemoryType::Semantic),
                        ),
                        perspective_core::types::Memory::Procedural(e) => (
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
            let resp = ErrorResponse {
                error: format!("Invalid request body: {e}"),
            };
            return (
                "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                serde_json::to_string(&resp).unwrap_or_default(),
            );
        }
    };

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
                            perspective_core::types::Memory::Episodic(e) => &e.base.content,
                            perspective_core::types::Memory::Semantic(e) => &e.base.content,
                            perspective_core::types::Memory::Procedural(e) => &e.base.content,
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
            let resp = TenantsApiResponse { tenants };
            (
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                serde_json::to_string(&resp).unwrap_or_default(),
            )
        }
        Err(e) => {
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

/// Try to load a config from file, falling back to default.
fn load_config(path: Option<&str>) -> Config {
    match path {
        Some(p) => {
            let data = match std::fs::read_to_string(p) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("Warning: could not read config at {}: {e}", p);
                    eprintln!("Using default config.");
                    return Config::default();
                }
            };
            match toml::from_str::<Config>(&data) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Warning: could not parse config: {e}");
                    eprintln!("Using default config.");
                    Config::default()
                }
            }
        }
        None => {
            // Check default locations
            let candidates: Vec<PathBuf> = vec![
                PathBuf::from("./perspective.toml"),
                PathBuf::from("./perspective-config.toml"),
            ]
            .into_iter()
            .chain(dirs_config_path())
            .collect();
            for p in &candidates {
                if p.exists() {
                    println!("Using config: {}", p.display());
                    return load_config(Some(p.to_string_lossy().as_ref()));
                }
            }
            Config::default()
        }
    }
}

fn dirs_config_path() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        Some(PathBuf::from(xdg).join("perspective").join("config.toml"))
    } else if let Ok(home) = std::env::var("HOME") {
        Some(
            PathBuf::from(home)
                .join(".config")
                .join("perspective")
                .join("config.toml"),
        )
    } else {
        None
    }
}

/// Build status from live engine data.
async fn build_status(engine: &PerspectiveEngine, config: &Config) -> StatusPayload {
    let status = engine.status_response();
    let activity = engine.get_activity(20);
    StatusPayload {
        health: status.health,
        uptime_secs: status.uptime_secs,
        total_memories: status.total_memories,
        tenant_count: 0,
        memory_types: MemoryTypeCounts {
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

fn print_status_table(status: &StatusPayload, config: &Config) {
    let health_icon = match status.health.as_str() {
        "healthy" => "🟢",
        "degraded" => "🟡",
        _ => "🔴",
    };

    println!();
    println!("  Perspective Engine Status");
    println!("  ══════════════════════════════════════");
    println!("  Health:            {} {}", health_icon, status.health);
    println!("  Uptime:            {}s", status.uptime_secs);
    println!();
    println!("  Memory Counts");
    println!("  ─────────────────────────────────────");
    println!("  Total:             {}", status.total_memories);
    println!("  Episodic:          {}", status.memory_types.episodic);
    println!("  Semantic:          {}", status.memory_types.semantic);
    println!("  Procedural:        {}", status.memory_types.procedural);
    println!("  GC Candidates:     {}", status.gc_candidates);
    println!();
    println!("  Tenants:           {}", status.tenant_count);
    println!();
    println!("  Decay Config");
    println!("  ─────────────────────────────────────");
    println!("  episodic_lambda:   {}", config.decay.episodic_lambda);
    println!("  semantic_lambda:   {}", config.decay.semantic_lambda);
    println!("  procedural_lambda: {}", config.decay.procedural_lambda);
    println!("  learning_rate:     {}", config.decay.learning_rate);
    println!("  retrieval_thresh:  {}", config.decay.retrieval_threshold);
    println!("  gc_threshold:      {}", config.decay.gc_threshold);
    println!();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Serve {
            host,
            dashboard_port,
            dashboard_dir,
        } => {
            let mut config = load_config(cli.config.as_deref());
            // Apply --data-dir override
            if let Some(ref d) = cli.data_dir {
                config.storage.data_dir = d.clone();
            }

            // Initialize the PerspectiveEngine (read-only: may skip graph if locked)
            let engine = match PerspectiveEngine::new_readonly(config.clone()) {
                Ok(e) => {
                    println!("  ✓ PerspectiveEngine initialized");
                    Arc::new(e)
                }
                Err(e) => {
                    eprintln!("  ⚠ Could not initialize PerspectiveEngine: {e}");
                    eprintln!("    API endpoints requiring the engine will return errors.");
                    eprintln!("    Dashboard and health check will still work.");
                    Arc::new(PerspectiveEngine::new_readonly(Config::default())?)
                }
            };

            let status_payload = build_status(&engine, &config).await;
            let _status_json = serde_json::to_string(&status_payload)?;

            // Static files from React dist (if --dashboard-dir provided)
            let static_files = dashboard_dir.and_then(|d| static_files::StaticFiles::new(d));

            println!("╔══════════════════════════════════════════╗");
            println!("║      Perspective Memory Engine           ║");
            println!("╚══════════════════════════════════════════╝");
            println!();
            if dashboard_port > 0 {
                println!("  Dashboard:    http://{host}:{dashboard_port}");
            }
            println!("  API:          http://{host}:{dashboard_port}/api/");
            println!("  Data dir:     {}", config.storage.data_dir.display());
            println!();
            println!("  ✓ Server ready. Listening...");

            // Start the extraction loop if enabled
            if config.extraction.enabled {
                let extraction_handle = engine.clone().start_extraction_loop();
                // Detach the handle so it runs in the background
                tokio::spawn(async move {
                    let _ = extraction_handle.await;
                });
                println!("  ✓ Extraction loop started (batch every {}s)", config.extraction.batch_interval_secs);
            }

            // Start the decay loop (hourly Ebbinghaus maintenance)
            {
                let decay_handle = engine.clone().start_decay_loop();
                tokio::spawn(async move {
                    let _ = decay_handle.await;
                });
                println!("  ✓ Decay loop started (hourly)");
            }

            // Start the consolidation loop (dedup, promotion, community detection)
            {
                let consolidation_handle = engine.clone().start_consolidation_loop();
                tokio::spawn(async move {
                    let _ = consolidation_handle.await;
                });
                println!("  ✓ Consolidation loop started (every {}s)", config.consolidation.interval_secs);
            }

            // If dashboard is enabled, serve via a tiny HTTP server
            if dashboard_port > 0 {
                let config_for_server = config.clone();
                let engine_for_server = engine.clone();
                let static_files_for_server = static_files;
                let dashboard_handle = tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    use tokio::net::TcpListener;

                    let addr = format!("{host}:{dashboard_port}");
                    let listener = match TcpListener::bind(&addr).await {
                        Ok(l) => l,
                        Err(e) => {
                            eprintln!("Failed to bind dashboard on {addr}: {e}");
                            return;
                        }
                    };
                    println!("  ✓ Dashboard serving on http://{addr}");

                    loop {
                        let (mut stream, _remote) = match listener.accept().await {
                            Ok(s) => s,
                            Err(_) => continue,
                        };

                        let mut buf = vec![0u8; 8192];
                        let n = match stream.read(&mut buf).await {
                            Ok(n) => n,
                            Err(_) => continue,
                        };
                        let request = String::from_utf8_lossy(&buf[..n]);

                        // ── Determine method and path ──────────────────────
                        let first_line = request.lines().next().unwrap_or("");
                        let parts: Vec<&str> = first_line.split_whitespace().collect();
                        let method = parts.first().copied().unwrap_or("");
                        let path = parts.get(1).copied().unwrap_or("/");

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
                            handle_tenants(&engine_for_server).await
                        } else if method == "GET" && path == "/api/status" {
                            let st = build_status(&engine_for_server, &config_for_server).await;
                            let json = serde_json::to_string(&st).unwrap_or_default();
                            (
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                                json,
                            )
                        } else if method == "POST" && path == "/api/store" {
                            let body_str = extract_body(&request).to_string();
                            handle_store(&engine_for_server, &body_str).await
                        } else if method == "POST" && path == "/api/recall" {
                            let body_str = extract_body(&request).to_string();
                            handle_recall(&engine_for_server, &body_str).await
                        } else if method == "POST" && path == "/api/reflect" {
                            let body_str = extract_body(&request).to_string();
                            handle_reflect(&engine_for_server, &body_str).await
                        } else if method == "GET" && path.starts_with("/api/activity") {
                            let resp = engine_for_server.get_activity(50);
                            let json = serde_json::to_string(&resp).unwrap_or_default();
                            (
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                                json,
                            )
                        } else if method == "GET" && path == "/api/processes" {
                            let resp = engine_for_server.get_processes();
                            let json = serde_json::to_string(&resp).unwrap_or_default();
                            (
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                                json,
                            )
                        } else if method == "GET" && path == "/api/graph" {
                            let resp = engine_for_server.get_graph_stats();
                            let json = serde_json::to_string(&resp).unwrap_or_default();
                            (
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\n".into(),
                                json,
                            )
                        } else if method == "GET" && path == "/api/config" {
                            let resp = engine_for_server.get_config_response();
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
                            let resp = engine_for_server.list_memories("hermes", &q, limit);
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
                            if let Some(ref sf) = static_files_for_server {
                                let (status, ct, body) = sf.serve(path);
                                (
                                    format!("{status}\r\nContent-Type: {ct}\r\nAccess-Control-Allow-Origin: *\r\n"),
                                    body,
                                )
                            } else {
                                // No dashboard dir configured, try embedded HTML fallback
                                if path == "/" || path == "/ " {
                                    (
                                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n".into(),
                                        dashboard::dashboard_html("{}"),
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
                });

                // Keep the server alive until Ctrl+C
                tokio::select! {
                    _ = dashboard_handle => {},
                    _ = tokio::signal::ctrl_c() => {
                        println!("\nShutting down...");
                    }
                }
            } else {
                tokio::signal::ctrl_c().await?;
                println!("\nShutting down...");
            }
        }

        Commands::Status { tenant, json } => {
            let config = load_config(cli.config.as_deref());

            // Create engine to read real data from Monitor
            let engine = match PerspectiveEngine::new(config.clone()) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("Could not initialize engine: {e}");
                    return Ok(());
                }
            };

            if let Some(tenant_id) = tenant {
                // Per-tenant status
                let activity = engine.get_activity(50);
                let events_for_tenant: Vec<_> = activity.events.iter().collect();
                if json {
                    let payload = serde_json::json!({
                        "tenant_id": tenant_id,
                        "health": "healthy",
                        "recent_events": events_for_tenant.len(),
                    });
                    println!("{}", serde_json::to_string_pretty(&payload)?);
                } else {
                    println!();
                    println!("  Tenant: {tenant_id}");
                    println!("  ────────────────────────────");
                    println!("  Health:       🟢 healthy");
                    println!("  Recent events: {}", events_for_tenant.len());
                    println!();
                }
                return Ok(());
            }

            let status = engine.status_response();
            let status_payload = StatusPayload {
                health: status.health,
                uptime_secs: status.uptime_secs,
                total_memories: status.total_memories,
                tenant_count: 0,
                memory_types: MemoryTypeCounts {
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
                recent_activity: vec![],
            };

            if json {
                println!("{}", serde_json::to_string_pretty(&status_payload)?);
            } else {
                print_status_table(&status_payload, &config);
            }
        }

        Commands::Init { dir, force } => {
            let data_dir = dir.unwrap_or_else(|| {
                config_dir_path().unwrap_or_else(|| PathBuf::from("./perspective-data"))
            });
            let config_path = data_dir.join("perspective.toml");

            if data_dir.exists() && !force {
                eprintln!("Data directory already exists: {}", data_dir.display());
                eprintln!("Use --force to overwrite.");
                return Ok(());
            }

            // Create directory tree
            std::fs::create_dir_all(&data_dir)?;
            std::fs::create_dir_all(data_dir.join("qdrant"))?;

            let config = Config::default();
            let toml_str = toml::to_string_pretty(&config)
                .map_err(|e| format!("Failed to serialize default config: {e}"))?;

            std::fs::write(&config_path, &toml_str)?;

            println!();
            println!("  ✓ Initialized Perspective data directory");
            println!("    Data dir:   {}", data_dir.display());
            println!("    Config:     {}", config_path.display());
            println!();
            println!("  Edit the config file to customize settings,");
            println!("  then run `perspective serve` to start the engine.");
            println!();
        }

        Commands::Config { json } => {
            let config = load_config(cli.config.as_deref());
            if json {
                println!("{}", serde_json::to_string_pretty(&config)?);
            } else {
                println!();
                println!("  Current Configuration");
                println!("  ══════════════════════════════════════");
                println!();
                match toml::to_string_pretty(&config) {
                    Ok(s) => println!("{s}"),
                    Err(_) => println!("{config:#?}"),
                }
                println!();
            }
        }
    }

    Ok(())
}

/// Default config directory path.
fn config_dir_path() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        Some(PathBuf::from(xdg).join("perspective"))
    } else if let Ok(home) = std::env::var("HOME") {
        Some(
            PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("perspective"),
        )
    } else {
        None
    }
}
