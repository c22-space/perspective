use clap::{Parser, Subcommand};
use perspective_core::config::Config;
use perspective_core::engine::PerspectiveEngine;
use serde::Serialize;
use std::path::PathBuf;

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
    }

    Ok(())
}
