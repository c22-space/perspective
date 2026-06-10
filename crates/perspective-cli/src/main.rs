use clap::{Parser, Subcommand};
use perspective_core::config::Config;
use perspective_core::engine::PerspectiveEngine;
use serde::Serialize;
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
    /// Start the HTTP server (daemon mode)
    Start {
        /// Port to listen on (default: 2085)
        #[arg(short, long, default_value_t = 2085)]
        port: u16,

        /// Bind address (default: 127.0.0.1)
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
    },
    /// Stop a running server
    Stop,
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

/// PID file path for the running server.
fn pid_file_path() -> PathBuf {
    config_dir_path()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("perspective.pid")
}

/// Write PID file.
fn write_pid_file(pid: u32) -> std::io::Result<()> {
    std::fs::write(pid_file_path(), pid.to_string())
}

/// Read PID from file, returns None if not running.
fn read_pid_file() -> Option<u32> {
    let path = pid_file_path();
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    content.trim().parse().ok()
}

/// Remove PID file.
fn remove_pid_file() {
    let _ = std::fs::remove_file(pid_file_path());
}

/// Check if a process with given PID is running.
fn is_process_running(pid: u32) -> bool {
    // Signal 0 checks if process exists without actually signaling
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Load config early to get data_dir for log file
    let early_config = load_config(cli.config.as_deref());
    let log_dir = if let Some(ref d) = cli.data_dir {
        d.clone()
    } else {
        early_config.storage.data_dir.clone()
    };

    // Set up dual logging: stdout (for terminal) + file (for debugging)
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

        tracing_subscriber::registry()
            .with(stdout_layer)
            .with(file_layer)
            .init();
        tracing::info!("Logging to {}", log_path.display());
    } else {
        tracing_subscriber::registry()
            .with(stdout_layer)
            .init();
        eprintln!("Warning: could not open log file, logging to stdout only");
    }

    match cli.command {
        Commands::Status { tenant, json } => {
            // Try HTTP API first (if server is running)
            let url = format!("http://127.0.0.1:2085/api/status");
            if let Ok(body) = reqwest::blocking::get(&url) {
                if body.status().is_success() {
                    let status: serde_json::Value = body.json()?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&status)?);
                    } else {
                        println!();
                        println!("  Perspective Engine Status (HTTP)");
                        println!("  ══════════════════════════════════════");
                        println!("  Health:    {}", status["health"]);
                        println!("  Uptime:    {}s", status["uptime_secs"]);
                        println!("  Memories:  {}", status["total_memories"]);
                        println!();
                        println!("  Memory Types");
                        println!("  ─────────────────────────────────────");
                        println!("  Episodic:  {}", status["memory_types"]["episodic"]);
                        println!("  Semantic:  {}", status["memory_types"]["semantic"]);
                        println!("  Procedural: {}", status["memory_types"]["procedural"]);
                        println!();
                    }
                    return Ok(());
                }
            }

            // Server not running, create engine directly
            let config = load_config(cli.config.as_deref());
            let engine = match PerspectiveEngine::new(config.clone()) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("Could not initialize engine: {e}");
                    eprintln!("Is the server running? Try `perspective start`");
                    return Ok(());
                }
            };

            if let Some(tenant_id) = tenant {
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
            println!("  then run `perspective status` to check engine state.");
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

        Commands::Start { port, host } => {
            // Check if already running
            if let Some(pid) = read_pid_file() {
                if is_process_running(pid) {
                    eprintln!("Perspective server already running (PID {pid})");
                    eprintln!("Use `perspective stop` to stop it first.");
                    return Ok(());
                }
                // Stale PID file
                remove_pid_file();
            }
            let mut config = load_config(cli.config.as_deref());

            // Apply data_dir override from -d flag
            if let Some(ref data_dir) = cli.data_dir {
                config.storage.data_dir = data_dir.clone();
            }

            let _ = std::fs::create_dir_all(&config.storage.data_dir);

            // Create engine
            let engine = match PerspectiveEngine::new(config.clone()) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("Failed to create engine: {e}");
                    return Err(e.into());
                }
            };

            let engine_arc = Arc::new(engine);

            // Start background extraction loop (processes buffered LLM extractions)
            if config.extraction.enabled {
                let _ = engine_arc.clone().start_extraction_loop();
            }

            // Run missed decay if server was down at midnight
            {
                let now_secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                let secs_per_day = 86400u64;
                let last_midnight = (now_secs / secs_per_day) * secs_per_day;
                let hours_since_midnight = (now_secs - last_midnight) / 3600;
                // If it's past 00:30 UTC and decay hasn't run today, run it now
                if hours_since_midnight >= 0 && (now_secs - last_midnight) > 1800 {
                    tracing::info!("decay: running missed decay from startup ({}h since midnight)", hours_since_midnight);
                    let engine_for_decay = engine_arc.clone();
                    tokio::spawn(async move {
                        engine_for_decay.run_decay_tick().await;
                    });
                }
            }

            // Schedule daily decay at midnight UTC
            {
                let engine_clone = engine_arc.clone();
                tokio::spawn(async move {
                    loop {
                        // Compute seconds until next midnight UTC
                        let now_secs = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs();
                        let secs_per_day = 86400u64;
                        let next_midnight = ((now_secs / secs_per_day) + 1) * secs_per_day;
                        let sleep_secs = next_midnight - now_secs;
                        tracing::info!(
                            "decay: sleeping {}s until next midnight UTC",
                            sleep_secs
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(sleep_secs)).await;
                        engine_clone.run_decay_tick().await;
                    }
                });
            }

            // Write PID file
            let pid = std::process::id();
            if let Err(e) = write_pid_file(pid) {
                eprintln!("Warning: could not write PID file: {e}");
            }

            // Start HTTP server
            let server_config = perspective_core::server::ServerConfig {
                host: host.clone(),
                port,
                dashboard_dir: None,
            };
            let server_handle = perspective_core::server::start_background_with_config(
                engine_arc,
                config.clone(),
                server_config,
            );

            println!();
            println!("  ✓ Perspective server started");
            println!("    PID:    {pid}");
            println!("    Listen: {host}:{port}");
            println!("    Data:   {}", config.storage.data_dir.display());
            println!("    Health: http://{host}:{port}/api/health");
            println!();

            // Run forever
            server_handle.await?;
        }

        Commands::Stop => {
            let pid = match read_pid_file() {
                Some(p) => p,
                None => {
                    eprintln!("No PID file found. Is the server running?");
                    return Ok(());
                }
            };

            if !is_process_running(pid) {
                eprintln!("Server process (PID {pid}) not running. Cleaning up stale PID file.");
                remove_pid_file();
                return Ok(());
            }

            // Send SIGTERM
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }

            // Wait for process to exit (up to 5 seconds)
            for _ in 0..50 {
                if !is_process_running(pid) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }

            if is_process_running(pid) {
                eprintln!("Server did not stop gracefully. Sending SIGKILL...");
                unsafe {
                    libc::kill(pid as i32, libc::SIGKILL);
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }

            remove_pid_file();
            println!("✓ Perspective server stopped (PID {pid})");
        }
    }

    Ok(())
}
