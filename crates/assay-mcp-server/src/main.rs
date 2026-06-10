use anyhow::Result;
use assay_mcp_server::config;
use assay_mcp_server::server::Server;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

// P61b: upstream proxy mode lives in the binary crate only (not exposed in the library's public API),
// because it is invoked solely from here. See docs/reference/mcp-upstream-proxy-mode.md.
mod proxy;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, default_value = "policies")]
    policy_root: PathBuf,
    #[command(subcommand)]
    mode: Option<Mode>,
}

#[derive(Subcommand, Debug)]
enum Mode {
    /// Run as an MCP upstream proxy (manifest-observation v0): forward a tiny method allowlist to a
    /// stdio upstream and deny everything else (tools/call included). This mode does NOT execute tool
    /// calls through the proxy and emits no manifest artifact yet. --policy-root is unused here.
    Proxy {
        /// The upstream MCP server command to spawn (stdio transport).
        #[arg(long)]
        upstream_command: String,
        /// Arguments passed to the upstream command (repeatable). Hyphen-led values are allowed so an
        /// upstream command can take its own flags (e.g. `--upstream-arg -u`).
        #[arg(long = "upstream-arg", allow_hyphen_values = true)]
        upstream_args: Vec<String>,
    },
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

    match args.mode {
        Some(Mode::Proxy {
            upstream_command,
            upstream_args,
        }) => {
            tracing::info!(
                event = "proxy_start",
                upstream_command = %upstream_command,
                mode = "manifest_observation_v0"
            );
            proxy::run(upstream_command, upstream_args).await
        }
        None => {
            tracing::info!(
                event = "server_start",
                policy_root = ?args.policy_root,
                config = ?cfg
            );
            Server::run(args.policy_root, cfg).await
        }
    }
}
