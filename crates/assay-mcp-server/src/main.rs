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
    /// calls through the proxy; it observes the upstream tools/list and (with --mcp-manifest-observed-out)
    /// emits manifest evidence only. --policy-root is unused here.
    Proxy {
        /// The upstream MCP server command to spawn (stdio transport).
        #[arg(long)]
        upstream_command: String,
        /// Arguments passed to the upstream command (repeatable). Hyphen-led values are allowed so an
        /// upstream command can take its own flags (e.g. `--upstream-arg -u`).
        #[arg(long = "upstream-arg", allow_hyphen_values = true)]
        upstream_args: Vec<String>,
        /// Write the observed tool manifest (assay.mcp_manifest_observed.v0) to this path at shutdown
        /// (and on each completed tools/list chain). When set and no tools/list is observed, a
        /// status:not_observed artifact is written — never an absent file.
        #[arg(long)]
        mcp_manifest_observed_out: Option<PathBuf>,
        /// Write a small proxy observation-health record (assay.proxy_observation_health.v0) to this
        /// path at shutdown: how complete the observation was, separate from the manifest itself.
        #[arg(long)]
        proxy_observation_health_out: Option<PathBuf>,
    },
    /// Run as an MCP upstream ENFORCING proxy (P61e-c): an explicit, separate run mode — a different
    /// risk class from `proxy`, never a variant of it. Every `tools/call` runs through the policy
    /// decision point (caller-allowance, credential-scope, drift); a call that clears every gate is
    /// forwarded, otherwise it is denied with the precedence-pinned reason of the first gate that fails.
    /// The handshake, `ping`, and `tools/list` still forward; other methods stay `proxy_unsupported`.
    /// A missing/unreadable/malformed `--enforce-policy` OR `--declared-mcp-manifest` fails startup
    /// (non-zero exit) — never a runtime deny: in enforcing mode both inputs are required.
    ProxyEnforce {
        /// The upstream MCP server command to spawn (stdio transport).
        #[arg(long)]
        upstream_command: String,
        /// Arguments passed to the upstream command (repeatable). Hyphen-led values are allowed.
        #[arg(long = "upstream-arg", allow_hyphen_values = true)]
        upstream_args: Vec<String>,
        /// Path to the enforce policy (YAML): the static `caller.id`, the upstream credential, and the
        /// caller's allowances. A missing/unreadable/malformed policy is a startup failure.
        #[arg(long)]
        enforce_policy: PathBuf,
        /// Path to the approved declared-manifest baseline (`assay.declared_mcp_manifest.v0`, JSON):
        /// the per-tool `tool_digest` the caller approved, against which the drift gate compares. Required
        /// in enforcing mode; a missing/unreadable/malformed/wrong-schema baseline is a startup failure.
        #[arg(long)]
        declared_mcp_manifest: PathBuf,
        /// Optional NDJSON path for the per-call `assay.enforcement_decision.v0` evidence record
        /// (P61e-d): one record per `tools/call` decision. The record is a policy decision only and
        /// never asserts the upstream side effect. An allowed call whose record cannot be written
        /// fails closed (it is not forwarded).
        #[arg(long)]
        enforcement_decision_out: Option<PathBuf>,
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
            mcp_manifest_observed_out,
            proxy_observation_health_out,
        }) => {
            tracing::info!(
                event = "proxy_start",
                upstream_command = %upstream_command,
                mode = "manifest_observation_v0"
            );
            proxy::run(
                upstream_command,
                upstream_args,
                proxy::Mode::Observe,
                proxy::enforce::EnforceInputs::default(),
                mcp_manifest_observed_out,
                proxy_observation_health_out,
            )
            .await
        }
        Some(Mode::ProxyEnforce {
            upstream_command,
            upstream_args,
            enforce_policy,
            declared_mcp_manifest,
            enforcement_decision_out,
        }) => {
            // Load + validate BOTH inputs BEFORE starting the proxy. A bad policy or baseline is a
            // misconfigured service: fail startup with a non-zero exit, never start an enforcing proxy
            // that cannot decide (and never degrade to a runtime deny).
            let policy = proxy::enforce::load(&enforce_policy)?;
            let baseline = proxy::enforce::load_declared_manifest(&declared_mcp_manifest)?;
            tracing::info!(
                event = "proxy_start",
                upstream_command = %upstream_command,
                mode = "enforce_pdp_c3",
                caller = %policy.caller.id,
                baseline_tools = baseline.tools.len()
            );
            proxy::run(
                upstream_command,
                upstream_args,
                proxy::Mode::Enforce,
                proxy::enforce::EnforceInputs {
                    policy: Some(policy),
                    baseline: Some(baseline),
                    decision_out: enforcement_decision_out,
                },
                None,
                None,
            )
            .await
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
