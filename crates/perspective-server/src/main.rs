use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "perspective", version, about = "Perspective memory engine")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the gRPC server
    Serve {
        #[arg(short, long, default_value = "127.0.0.1")]
        host: String,
        #[arg(short, long, default_value = "50051")]
        port: u16,
        #[arg(short, long)]
        config: Option<String>,
    },
    /// Show engine status
    Status {
        #[arg(short, long)]
        tenant: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { host: _, port: _, config: _ } => {
            println!("Starting Perspective server");
            // TODO: start gRPC server
        }
        Commands::Status { tenant: _, json: _ } => {
            println!("Perspective status");
            // TODO: show status
        }
    }

    Ok(())
}
