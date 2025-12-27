use anyhow::Result;
use assay_mcp_server::config;
use assay_mcp_server::server::Server;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, default_value = "policies")]
    policy_root: PathBuf,
}

use tracing_subscriber::{fmt, EnvFilter};

fn init_logging(log_level: &str) {
    let filter = EnvFilter::try_new(log_level).unwrap_or_else(|_| EnvFilter::new("info"));

    fmt()
        .with_env_filter(filter)
        .json()
        .with_timer(fmt::time::UtcTime::rfc_3339())
        .with_target(true)
        .with_current_span(false)
        .with_span_list(false)
        .with_writer(std::io::stderr) // Explicitly stderr
        .init();
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    // Do not use eprintln here, use tracing after init
    // But config loads from env first.
    let cfg = config::ServerConfig::from_env();

    init_logging(&cfg.log_level);

    tracing::info!(
        event = "server_start",
        policy_root = ?args.policy_root,
        config = ?cfg
    );

    Server::run(args.policy_root, cfg).await
}
