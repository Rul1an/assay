use super::super::args::{McpArgs, McpSub, McpWrapArgs};
// use super::exit_codes;
use assay_core::mcp::policy::McpPolicy;
use assay_core::mcp::proxy::McpProxy;

pub async fn run(args: McpArgs) -> anyhow::Result<i32> {
    match args.cmd {
        McpSub::Wrap(wrap_args) => cmd_wrap(wrap_args).await,
        McpSub::ConfigPath(config_args) => {
            super::config_path::run(config_args);
            Ok(0)
        }
    }
}

async fn cmd_wrap(args: McpWrapArgs) -> anyhow::Result<i32> {
    if args.command.is_empty() {
        anyhow::bail!("No command specified. Usage: assay mcp wrap -- <cmd> [args]");
    }

    let cmd = &args.command[0];
    let cmd_args = &args.command[1..];

    // Load Policy
    let policy = if args.policy.exists() {
        eprintln!("[assay] loading policy from {}", args.policy.display());
        McpPolicy::from_file(&args.policy)?
    } else {
        anyhow::bail!(
            "Policy file not found at {}. Aborting instead of using insecure default (allow-all) policy.",
            args.policy.display()
        );
    };

    let config = assay_core::mcp::proxy::ProxyConfig {
        dry_run: args.dry_run,
        verbose: args.verbose,
        audit_log_path: args.audit_log,
    };

    if config.dry_run {
        eprintln!("[assay] DRY RUN MODE: No actions will be blocked.");
    }

    eprintln!("[assay] wrapping command: {} {:?}", cmd, cmd_args);

    // Spawn proxy
    let proxy = McpProxy::spawn(cmd, cmd_args, policy, config)?;

    // Run (blocking for now, as it manages threads)
    let code = tokio::task::spawn_blocking(move || proxy.run()).await??;

    Ok(code)
}
